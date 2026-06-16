use anyhow::Result;
use opentelemetry::{
    Context, KeyValue,
    trace::{
        Span, SpanContext, SpanId as OtelSpanId, SpanKind, Status, TraceContextExt, TraceFlags,
        TraceId as OtelTraceId, TraceState, Tracer, TracerProvider as _,
    },
};
use opentelemetry_otlp::{SpanExporter, WithExportConfig};
use opentelemetry_sdk::{
    Resource,
    trace as sdktrace,
    trace::{IdGenerator, RandomIdGenerator, SdkTracerProvider},
};
use std::cell::Cell;
use trace_weft_core::{SpanId, SpanRecord, SpanStatus, TraceId, TraceWeftSpanKind};

/// Map a TraceWeft [`TraceId`] (128-bit UUID) onto an OpenTelemetry trace ID.
///
/// Both are 128-bit, so the UUID's 16 bytes are copied verbatim — the mapping
/// is lossless and bijective, and identical inputs always produce identical
/// outputs.
pub fn otel_trace_id(id: TraceId) -> OtelTraceId {
    OtelTraceId::from_bytes(*id.0.as_bytes())
}

/// Map a TraceWeft [`SpanId`] (128-bit UUID) onto an OpenTelemetry span ID
/// (64-bit).
///
/// OTel span IDs are only 8 bytes, so we keep the **low** 8 bytes of the UUID
/// and drop the high 8. TraceWeft mints UUIDv7s, whose layout is a 48-bit
/// millisecond timestamp + version/variant bits in the high half and the
/// random `rand_b` field in the low half. Keeping the low bytes therefore
/// preserves the most entropy and minimizes collisions between sibling spans
/// (which share the high timestamp bits). The reduction is deterministic: the
/// same UUID always yields the same span ID.
pub fn otel_span_id(id: SpanId) -> OtelSpanId {
    let bytes = id.0.as_bytes();
    let mut low = [0u8; 8];
    low.copy_from_slice(&bytes[8..16]);
    OtelSpanId::from_bytes(low)
}

thread_local! {
    /// Per-thread slot the [`MappedIdGenerator`] reads when the SDK asks for the
    /// next trace/span ID. Set immediately before `SpanBuilder::start_with_context`
    /// (a synchronous call) and cleared right after.
    static NEXT_IDS: Cell<Option<(OtelTraceId, OtelSpanId)>> = const { Cell::new(None) };
}

/// An [`IdGenerator`] that yields caller-supplied IDs instead of random ones.
///
/// The OpenTelemetry SDK derives a span's own trace/span IDs from the
/// configured generator (it ignores `SpanBuilder::trace_id`/`span_id`), so this
/// is the supported hook for reconstructing the original trace tree on export.
/// When no IDs are staged it falls back to the standard random generator.
#[derive(Debug, Default)]
struct MappedIdGenerator {
    fallback: RandomIdGenerator,
}

impl IdGenerator for MappedIdGenerator {
    fn new_trace_id(&self) -> OtelTraceId {
        match NEXT_IDS.with(|c| c.get()) {
            Some((trace_id, _)) => trace_id,
            None => self.fallback.new_trace_id(),
        }
    }

    fn new_span_id(&self) -> OtelSpanId {
        match NEXT_IDS.with(|c| c.get()) {
            Some((_, span_id)) => span_id,
            None => self.fallback.new_span_id(),
        }
    }
}

/// Reconstruct and finish an OTel span for `record` on `tracer`, mapping the
/// TraceWeft trace/span/parent IDs onto OTel IDs so the exported span keeps its
/// place in the original trace tree.
fn export_with_tracer(tracer: &sdktrace::SdkTracer, record: &SpanRecord) {
    let start_time = std::time::UNIX_EPOCH + std::time::Duration::from_millis(record.start_time);
    let end_time = record
        .end_time
        .map(|t| std::time::UNIX_EPOCH + std::time::Duration::from_millis(t))
        .unwrap_or_else(std::time::SystemTime::now);

    let trace_id = otel_trace_id(record.trace_id);
    let span_id = otel_span_id(record.span_id);

    let mut builder = tracer.span_builder(record.name.clone());
    builder.span_kind = Some(map_otel_span_kind(record.span_kind));
    builder.start_time = Some(start_time);
    builder.attributes = Some(map_otel_attributes(record));

    // A span with a parent reconstructs the edge from a remote parent context
    // (the SDK takes the child's trace ID from there); a root span starts from
    // an empty context. Either way the span's own IDs come from the generator
    // below via the thread-local slot.
    let parent_cx = match record.parent_span_id {
        Some(parent) => {
            let parent_ctx = SpanContext::new(
                trace_id,
                otel_span_id(parent),
                TraceFlags::SAMPLED,
                true,
                TraceState::default(),
            );
            Context::new().with_remote_span_context(parent_ctx)
        }
        None => Context::new(),
    };

    NEXT_IDS.with(|c| c.set(Some((trace_id, span_id))));
    let mut span = builder.start_with_context(tracer, &parent_cx);
    NEXT_IDS.with(|c| c.set(None));

    let status = map_otel_status(record);
    if !matches!(status, Status::Unset) {
        span.set_status(status);
    }

    span.end_with_timestamp(end_time);
}

/// Map a TraceWeft span kind onto the closest OpenTelemetry span kind.
pub fn map_otel_span_kind(kind: TraceWeftSpanKind) -> SpanKind {
    match kind {
        TraceWeftSpanKind::LlmCall => SpanKind::Client,
        TraceWeftSpanKind::Tool => SpanKind::Internal,
        TraceWeftSpanKind::Retrieval => SpanKind::Client,
        _ => SpanKind::Internal,
    }
}

/// Map a TraceWeft span status onto an OpenTelemetry status.
///
/// Statuses without an OTel equivalent (in progress, skipped, cancelled,
/// pending approval) map to `Status::Unset`.
pub fn map_otel_status(record: &SpanRecord) -> Status {
    match record.status {
        SpanStatus::Error => Status::Error {
            description: record
                .error_message_redacted
                .clone()
                .unwrap_or_default()
                .into(),
        },
        SpanStatus::Ok => Status::Ok,
        _ => Status::Unset,
    }
}

/// Build OTel GenAI semantic-convention attributes plus TraceWeft-namespaced
/// attributes for a span record. Absent fields produce no attributes.
pub fn map_otel_attributes(record: &SpanRecord) -> Vec<KeyValue> {
    let mut attributes = Vec::new();

    // GenAI Semantic Conventions mapping
    if let Some(provider) = &record.model_provider {
        attributes.push(KeyValue::new("gen_ai.provider.name", provider.clone()));
    }
    if let Some(model) = &record.model_name {
        attributes.push(KeyValue::new("gen_ai.request.model", model.clone()));
    }
    if let Some(tool) = &record.tool_name {
        attributes.push(KeyValue::new("gen_ai.tool.name", tool.clone()));
    }

    // Custom TraceWeft attributes
    attributes.push(KeyValue::new(
        "trace_weft.span.kind",
        format!("{:?}", record.span_kind),
    ));

    if let Some(usage) = &record.token_usage {
        attributes.push(KeyValue::new(
            "gen_ai.usage.input_tokens",
            usage.input as i64,
        ));
        attributes.push(KeyValue::new(
            "gen_ai.usage.output_tokens",
            usage.output as i64,
        ));
        if let Some(reasoning) = usage.reasoning {
            attributes.push(KeyValue::new(
                "gen_ai.usage.reasoning_tokens",
                reasoning as i64,
            ));
        }
    }

    attributes
}

pub struct OtelExporter {
    provider: SdkTracerProvider,
    tracer: sdktrace::SdkTracer,
}

impl OtelExporter {
    pub fn new(endpoint: &str, service_name: &str) -> Result<Self> {
        let exporter = SpanExporter::builder()
            .with_tonic()
            .with_endpoint(endpoint)
            .build()?;

        let provider = SdkTracerProvider::builder()
            .with_batch_exporter(exporter)
            .with_id_generator(MappedIdGenerator::default())
            .with_resource(
                Resource::builder()
                    .with_service_name(service_name.to_string())
                    .build(),
            )
            .build();

        let tracer = provider.tracer("trace-weft");

        Ok(Self { provider, tracer })
    }

    pub fn export_span(&self, record: &SpanRecord) {
        export_with_tracer(&self.tracer, record);
    }

    pub fn shutdown(&self) {
        let _ = self.provider.shutdown();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use opentelemetry::Value;
    use opentelemetry_sdk::trace::InMemorySpanExporter;
    use trace_weft_core::test_util::{sample_span_full, sample_span_minimal};
    use uuid::Uuid;

    fn attr_value<'a>(attrs: &'a [KeyValue], key: &str) -> Option<&'a Value> {
        attrs
            .iter()
            .find(|kv| kv.key.as_str() == key)
            .map(|kv| &kv.value)
    }

    #[test]
    fn llm_and_retrieval_spans_map_to_client_kind() {
        assert_eq!(
            map_otel_span_kind(TraceWeftSpanKind::LlmCall),
            SpanKind::Client
        );
        assert_eq!(
            map_otel_span_kind(TraceWeftSpanKind::Retrieval),
            SpanKind::Client
        );
    }

    #[test]
    fn other_spans_map_to_internal_kind() {
        for kind in [
            TraceWeftSpanKind::Tool,
            TraceWeftSpanKind::Agent,
            TraceWeftSpanKind::Workflow,
            TraceWeftSpanKind::Memory,
            TraceWeftSpanKind::Guardrail,
        ] {
            assert_eq!(map_otel_span_kind(kind), SpanKind::Internal);
        }
    }

    #[test]
    fn maps_genai_semantic_attributes() {
        let attrs = map_otel_attributes(&sample_span_full());
        assert_eq!(
            attr_value(&attrs, "gen_ai.provider.name"),
            Some(&Value::from("openai"))
        );
        assert_eq!(
            attr_value(&attrs, "gen_ai.request.model"),
            Some(&Value::from("gpt-4.1"))
        );
        assert_eq!(
            attr_value(&attrs, "gen_ai.tool.name"),
            Some(&Value::from("kb_search"))
        );
        assert_eq!(
            attr_value(&attrs, "gen_ai.usage.input_tokens"),
            Some(&Value::I64(100))
        );
        assert_eq!(
            attr_value(&attrs, "gen_ai.usage.output_tokens"),
            Some(&Value::I64(50))
        );
        assert_eq!(
            attr_value(&attrs, "gen_ai.usage.reasoning_tokens"),
            Some(&Value::I64(25))
        );
    }

    #[test]
    fn always_tags_trace_weft_span_kind() {
        let full = map_otel_attributes(&sample_span_full());
        assert_eq!(
            attr_value(&full, "trace_weft.span.kind"),
            Some(&Value::from("LlmCall"))
        );

        let minimal = map_otel_attributes(&sample_span_minimal());
        assert_eq!(
            attr_value(&minimal, "trace_weft.span.kind"),
            Some(&Value::from("Tool"))
        );
    }

    #[test]
    fn omits_attributes_for_absent_fields() {
        let attrs = map_otel_attributes(&sample_span_minimal());
        assert!(attr_value(&attrs, "gen_ai.provider.name").is_none());
        assert!(attr_value(&attrs, "gen_ai.request.model").is_none());
        assert!(attr_value(&attrs, "gen_ai.tool.name").is_none());
        assert!(attr_value(&attrs, "gen_ai.usage.input_tokens").is_none());
        assert_eq!(attrs.len(), 1, "only trace_weft.span.kind expected");
    }

    #[test]
    fn ok_status_maps_to_otel_ok() {
        let mut record = sample_span_minimal();
        record.status = SpanStatus::Ok;
        assert!(matches!(map_otel_status(&record), Status::Ok));
    }

    #[test]
    fn error_status_carries_redacted_message() {
        let mut record = sample_span_minimal();
        record.status = SpanStatus::Error;
        record.error_message_redacted = Some("boom".into());
        match map_otel_status(&record) {
            Status::Error { description } => assert_eq!(description, "boom"),
            other => panic!("expected error status, got {other:?}"),
        }
    }

    #[test]
    fn non_terminal_statuses_map_to_unset() {
        for status in [
            SpanStatus::InProgress,
            SpanStatus::Skipped,
            SpanStatus::Cancelled,
            SpanStatus::PendingApproval,
        ] {
            let mut record = sample_span_minimal();
            record.status = status;
            assert!(matches!(map_otel_status(&record), Status::Unset));
        }
    }

    #[test]
    fn trace_id_reduction_is_lossless_and_deterministic() {
        let uuid = Uuid::from_u128(0x0123_4567_89ab_cdef_fedc_ba98_7654_3210);
        let id = TraceId(uuid);
        // Bytes are copied verbatim.
        assert_eq!(otel_trace_id(id).to_bytes(), *uuid.as_bytes());
        // Stable across calls.
        assert_eq!(otel_trace_id(id), otel_trace_id(id));
        // Distinct inputs stay distinct (bijective).
        assert_ne!(otel_trace_id(id), otel_trace_id(TraceId(Uuid::from_u128(1))));
    }

    #[test]
    fn span_id_reduction_keeps_low_eight_bytes_and_is_deterministic() {
        let uuid = Uuid::from_u128(0x0011_2233_4455_6677_8899_aabb_ccdd_eeff);
        let id = SpanId(uuid);
        let expected = {
            let mut low = [0u8; 8];
            low.copy_from_slice(&uuid.as_bytes()[8..16]);
            low
        };
        assert_eq!(otel_span_id(id).to_bytes(), expected);
        assert_eq!(expected, [0x88, 0x99, 0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff]);
        // Stable across calls.
        assert_eq!(otel_span_id(id), otel_span_id(id));
    }

    /// Two UUIDv7 IDs minted in the same millisecond share their high
    /// (timestamp) bytes; the low-byte reduction must still tell them apart.
    #[test]
    fn span_id_reduction_distinguishes_sibling_v7_ids() {
        let a = SpanId::new();
        let b = SpanId::new();
        assert_ne!(a, b);
        assert_ne!(otel_span_id(a), otel_span_id(b));
    }

    fn in_memory_tracer() -> (sdktrace::SdkTracer, InMemorySpanExporter) {
        let exporter = InMemorySpanExporter::default();
        let provider = SdkTracerProvider::builder()
            .with_simple_exporter(exporter.clone())
            .with_id_generator(MappedIdGenerator::default())
            .build();
        let tracer = provider.tracer("trace-weft-test");
        (tracer, exporter)
    }

    #[test]
    fn parent_child_export_shares_trace_id_and_links_parent() {
        let (tracer, exporter) = in_memory_tracer();

        let mut parent = sample_span_minimal();
        parent.parent_span_id = None;
        // sample_span_minimal() reuses fixed IDs, so give the child its own
        // span id; it shares the parent's trace and points back at the parent.
        let mut child = sample_span_minimal();
        child.trace_id = parent.trace_id;
        child.span_id = SpanId(Uuid::from_u128(0x0999));
        child.parent_span_id = Some(parent.span_id);

        export_with_tracer(&tracer, &parent);
        export_with_tracer(&tracer, &child);

        let spans = exporter.get_finished_spans().unwrap();
        assert_eq!(spans.len(), 2);

        let exported_parent = spans
            .iter()
            .find(|s| s.span_context.span_id() == otel_span_id(parent.span_id))
            .expect("parent span exported");
        let exported_child = spans
            .iter()
            .find(|s| s.span_context.span_id() == otel_span_id(child.span_id))
            .expect("child span exported");

        // Shared trace id, mapped from the TraceWeft trace id.
        assert_eq!(
            exported_parent.span_context.trace_id(),
            otel_trace_id(parent.trace_id)
        );
        assert_eq!(
            exported_child.span_context.trace_id(),
            exported_parent.span_context.trace_id()
        );
        // The child points back at the parent.
        assert_eq!(exported_child.parent_span_id, otel_span_id(parent.span_id));
        // The root has no parent.
        assert_eq!(exported_parent.parent_span_id, OtelSpanId::INVALID);
    }

    #[test]
    fn exported_span_carries_mapped_ids_and_name() {
        let (tracer, exporter) = in_memory_tracer();
        let record = sample_span_full();
        export_with_tracer(&tracer, &record);

        let spans = exporter.get_finished_spans().unwrap();
        assert_eq!(spans.len(), 1);
        let span = &spans[0];
        assert_eq!(span.span_context.trace_id(), otel_trace_id(record.trace_id));
        assert_eq!(span.span_context.span_id(), otel_span_id(record.span_id));
        assert_eq!(span.name, record.name);
    }
}
