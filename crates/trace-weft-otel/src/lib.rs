use anyhow::Result;
use opentelemetry::{
    KeyValue,
    trace::{Span, SpanKind, Status, Tracer, TracerProvider as _},
};
use opentelemetry_otlp::{SpanExporter, WithExportConfig};
use opentelemetry_sdk::{Resource, trace as sdktrace, trace::SdkTracerProvider};
use trace_weft_core::{SpanRecord, SpanStatus, TraceWeftSpanKind};

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
        let start_time =
            std::time::UNIX_EPOCH + std::time::Duration::from_millis(record.start_time);
        let end_time = record
            .end_time
            .map(|t| std::time::UNIX_EPOCH + std::time::Duration::from_millis(t))
            .unwrap_or_else(std::time::SystemTime::now);

        let mut builder = self.tracer.span_builder(record.name.clone());
        builder.span_kind = Some(map_otel_span_kind(record.span_kind));
        builder.start_time = Some(start_time);

        // TODO: Proper TraceId and SpanId propagation

        builder.attributes = Some(map_otel_attributes(record));

        let mut span = builder.start(&self.tracer);

        let status = map_otel_status(record);
        if !matches!(status, Status::Unset) {
            span.set_status(status);
        }

        span.end_with_timestamp(end_time);
    }

    pub fn shutdown(&self) {
        let _ = self.provider.shutdown();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use opentelemetry::Value;
    use trace_weft_core::test_util::{sample_span_full, sample_span_minimal};

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
}
