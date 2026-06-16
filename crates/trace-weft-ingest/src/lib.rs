//! OTLP/HTTP JSON trace ingestion.
//!
//! Payloads are decoded with the generated `opentelemetry-proto` types rather
//! than hand-walked `serde_json::Value`s, so the OTLP wire details (camelCase
//! fields, hex-encoded IDs, nanosecond timestamps as strings) are handled by
//! the proto crate's serde support. Original trace/span/parent IDs are
//! preserved; a malformed body is rejected with `400` instead of silently
//! substituting fresh IDs.

use std::collections::HashMap;

use axum::{
    Json, Router, body::Bytes, extract::State, http::StatusCode, response::IntoResponse,
    routing::post,
};
use opentelemetry_proto::tonic::collector::trace::v1::{
    ExportTraceServiceRequest, ExportTraceServiceResponse,
};
use opentelemetry_proto::tonic::common::v1::{KeyValue, any_value::Value as AnyValueEnum};
use opentelemetry_proto::tonic::trace::v1::{Span, status::StatusCode as OtlpStatusCode};
use trace_weft_core::{
    CapturePolicy, RunId, SpanId, SpanRecord, SpanStatus, TraceId, TraceWeftSpanKind, semconv,
};
use trace_weft_recorder::TraceStore;
use uuid::Uuid;

#[derive(Clone)]
pub struct IngestState {
    pub store: std::sync::Arc<dyn TraceStore>,
}

pub fn ingest_router(state: IngestState) -> Router {
    Router::new()
        .route("/v1/traces", post(handle_otlp_json_traces))
        .with_state(state)
}

/// OTLP/HTTP JSON traces endpoint.
///
/// Note: spans are recorded synchronously, one round-trip to the store per
/// span. That is fine for the local-first scale this targets; a
/// high-throughput deployment would batch inserts and add back-pressure here.
async fn handle_otlp_json_traces(
    State(state): State<IngestState>,
    body: Bytes,
) -> Result<impl IntoResponse, StatusCode> {
    let request: ExportTraceServiceRequest = serde_json::from_slice(&body).map_err(|e| {
        tracing::warn!("rejecting malformed OTLP payload: {e}");
        StatusCode::BAD_REQUEST
    })?;

    // One run groups every span in this export request.
    let run_id = RunId::new();

    for resource_spans in &request.resource_spans {
        for scope_spans in &resource_spans.scope_spans {
            for span in &scope_spans.spans {
                let record = span_to_record(span, run_id);
                if let Err(e) = state.store.record_span(record).await {
                    tracing::error!("failed to record ingested span: {e}");
                    return Err(StatusCode::INTERNAL_SERVER_ERROR);
                }
            }
        }
    }

    Ok(Json(ExportTraceServiceResponse::default()))
}

/// Convert a 16-byte OTLP trace id into a [`TraceId`]. Returns `None` for the
/// wrong length or the all-zero (invalid) id.
fn trace_id_from_bytes(bytes: &[u8]) -> Option<TraceId> {
    let arr: [u8; 16] = bytes.try_into().ok()?;
    arr.iter().any(|b| *b != 0).then(|| TraceId(Uuid::from_bytes(arr)))
}

/// Convert an 8-byte OTLP span id into a [`SpanId`], placing the bytes in the
/// low half of the UUID. This matches the reduction `trace-weft-otel` and
/// `trace-weft-mcp` apply when emitting 64-bit ids, so a span exported by
/// TraceWeft and re-ingested keeps a stable id. Returns `None` for the wrong
/// length or the all-zero (invalid) id.
fn span_id_from_bytes(bytes: &[u8]) -> Option<SpanId> {
    let low: [u8; 8] = bytes.try_into().ok()?;
    if low.iter().all(|b| *b == 0) {
        return None;
    }
    let mut full = [0u8; 16];
    full[8..16].copy_from_slice(&low);
    Some(SpanId(Uuid::from_bytes(full)))
}

/// Parse a `trace_weft.span.kind` attribute (the Rust variant name TraceWeft
/// exports, e.g. `LlmCall`) back into the enum.
fn parse_span_kind(name: &str) -> Option<TraceWeftSpanKind> {
    use TraceWeftSpanKind::*;
    Some(match name {
        "Workflow" => Workflow,
        "Agent" => Agent,
        "LlmCall" => LlmCall,
        "Embedding" => Embedding,
        "Retrieval" => Retrieval,
        "Rerank" => Rerank,
        "Tool" => Tool,
        "Memory" => Memory,
        "State" => State,
        "Planner" => Planner,
        "Router" => Router,
        "Guardrail" => Guardrail,
        "Evaluator" => Evaluator,
        "Handoff" => Handoff,
        "Checkpoint" => Checkpoint,
        "Replay" => Replay,
        "Error" => Error,
        _ => return None,
    })
}

/// Flatten an OTLP `AnyValue` into a plain JSON value for storage.
fn any_value_to_json(kv: &KeyValue) -> serde_json::Value {
    match kv.value.as_ref().and_then(|v| v.value.as_ref()) {
        Some(AnyValueEnum::StringValue(s)) => serde_json::json!(s),
        Some(AnyValueEnum::BoolValue(b)) => serde_json::json!(b),
        Some(AnyValueEnum::IntValue(i)) => serde_json::json!(i),
        Some(AnyValueEnum::DoubleValue(d)) => serde_json::json!(d),
        _ => serde_json::Value::Null,
    }
}

fn string_value(kv: &KeyValue) -> Option<String> {
    match kv.value.as_ref().and_then(|v| v.value.as_ref()) {
        Some(AnyValueEnum::StringValue(s)) => Some(s.clone()),
        _ => None,
    }
}

/// Map a decoded OTLP [`Span`] onto a TraceWeft [`SpanRecord`], preserving the
/// original trace/span/parent IDs.
fn span_to_record(span: &Span, run_id: RunId) -> SpanRecord {
    let trace_id = trace_id_from_bytes(&span.trace_id).unwrap_or_else(|| {
        tracing::warn!("OTLP span missing/invalid trace_id; synthesizing one");
        TraceId::new()
    });
    let span_id = span_id_from_bytes(&span.span_id).unwrap_or_else(|| {
        tracing::warn!("OTLP span missing/invalid span_id; synthesizing one");
        SpanId::new()
    });
    let parent_span_id = span_id_from_bytes(&span.parent_span_id);

    let mut otel_attributes = HashMap::new();
    let mut model_provider = None;
    let mut model_name = None;
    let mut tool_name = None;
    let mut span_kind = TraceWeftSpanKind::Workflow;

    for attr in &span.attributes {
        otel_attributes.insert(attr.key.clone(), any_value_to_json(attr));
        match attr.key.as_str() {
            semconv::GEN_AI_PROVIDER_NAME => model_provider = string_value(attr),
            semconv::GEN_AI_REQUEST_MODEL => model_name = string_value(attr),
            semconv::GEN_AI_TOOL_NAME => tool_name = string_value(attr),
            semconv::TRACE_WEFT_SPAN_KIND => {
                if let Some(kind) = string_value(attr).as_deref().and_then(parse_span_kind) {
                    span_kind = kind;
                }
            }
            _ => {}
        }
    }

    let (status, error_message_redacted) = match span.status.as_ref() {
        Some(s) if s.code == OtlpStatusCode::Error as i32 => (
            SpanStatus::Error,
            (!s.message.is_empty()).then(|| s.message.clone()),
        ),
        _ => (SpanStatus::Ok, None),
    };

    let start_time = span.start_time_unix_nano / 1_000_000;
    let end_time = (span.end_time_unix_nano != 0).then_some(span.end_time_unix_nano / 1_000_000);
    let latency_ms = (span.end_time_unix_nano > span.start_time_unix_nano)
        .then(|| (span.end_time_unix_nano - span.start_time_unix_nano) / 1_000_000);

    SpanRecord {
        trace_id,
        span_id,
        parent_span_id,
        run_id,
        session_id: None,
        user_id_hash: None,
        project_id: None,
        span_kind,
        name: span.name.clone(),
        start_time,
        end_time,
        status,
        status_message: None,
        error_type: None,
        error_message_redacted,
        attributes: HashMap::new(),
        otel_attributes,
        openinference_attributes: HashMap::new(),
        memory_state: None,
        input_ref: None,
        output_ref: None,
        prompt_template_id: None,
        prompt_version: None,
        model_provider,
        model_name,
        tool_name,
        tool_schema_hash: None,
        retrieval_query_hash: None,
        retrieved_document_refs: vec![],
        token_usage: None,
        cost_estimate: None,
        latency_ms,
        retry_count: None,
        cache_hit: None,
        redaction_policy: CapturePolicy::MetadataOnly,
        schema_version: "1.0".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use opentelemetry_proto::tonic::common::v1::AnyValue;

    fn hex_trace() -> String {
        // 16 bytes / 32 hex chars.
        "0123456789abcdef0011223344556677".to_string()
    }

    fn hex_span() -> String {
        // 8 bytes / 16 hex chars.
        "8899aabbccddeeff".to_string()
    }

    fn payload(extra_span_fields: &str) -> String {
        format!(
            r#"{{
                "resourceSpans": [{{
                    "scopeSpans": [{{
                        "spans": [{{
                            "traceId": "{}",
                            "spanId": "{}",
                            "name": "draft_answer",
                            "startTimeUnixNano": "1715000000000000000",
                            "endTimeUnixNano": "1715000005000000000"
                            {}
                        }}]
                    }}]
                }}]
            }}"#,
            hex_trace(),
            hex_span(),
            extra_span_fields
        )
    }

    fn parse_one(json: &str) -> SpanRecord {
        let req: ExportTraceServiceRequest = serde_json::from_str(json).unwrap();
        let span = &req.resource_spans[0].scope_spans[0].spans[0];
        span_to_record(span, RunId::new())
    }

    #[test]
    fn preserves_original_trace_and_span_ids() {
        let record = parse_one(&payload(""));
        // trace id = the 16 bytes verbatim.
        assert_eq!(
            record.trace_id.0,
            Uuid::parse_str("0123456789abcdef0011223344556677").unwrap()
        );
        // span id = the 8 bytes in the low half of the UUID.
        assert_eq!(&record.span_id.0.as_bytes()[8..16], &[
            0x88, 0x99, 0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff
        ]);
        assert_eq!(&record.span_id.0.as_bytes()[0..8], &[0u8; 8]);
    }

    #[test]
    fn converts_nanoseconds_to_millis() {
        let record = parse_one(&payload(""));
        assert_eq!(record.start_time, 1_715_000_000_000);
        assert_eq!(record.end_time, Some(1_715_000_005_000));
        assert_eq!(record.latency_ms, Some(5_000));
    }

    #[test]
    fn maps_genai_attributes_and_span_kind() {
        let attrs = r#",
            "attributes": [
                {"key": "gen_ai.provider.name", "value": {"stringValue": "openai"}},
                {"key": "gen_ai.request.model", "value": {"stringValue": "gpt-4.1"}},
                {"key": "trace_weft.span.kind", "value": {"stringValue": "LlmCall"}}
            ]"#;
        let record = parse_one(&payload(attrs));
        assert_eq!(record.model_provider.as_deref(), Some("openai"));
        assert_eq!(record.model_name.as_deref(), Some("gpt-4.1"));
        assert_eq!(record.span_kind, TraceWeftSpanKind::LlmCall);
        assert_eq!(
            record.otel_attributes.get("gen_ai.request.model"),
            Some(&serde_json::json!("gpt-4.1"))
        );
    }

    #[test]
    fn maps_error_status_with_message() {
        let status = r#",
            "status": {"code": 2, "message": "boom"}"#;
        let record = parse_one(&payload(status));
        assert_eq!(record.status, SpanStatus::Error);
        assert_eq!(record.error_message_redacted.as_deref(), Some("boom"));
    }

    #[test]
    fn root_span_has_no_parent() {
        let record = parse_one(&payload(""));
        assert!(record.parent_span_id.is_none());
    }

    #[test]
    fn preserves_parent_span_id_when_present() {
        let with_parent = r#","parentSpanId": "1122334455667788""#;
        let record = parse_one(&payload(with_parent));
        let parent = record.parent_span_id.expect("parent present");
        assert_eq!(&parent.0.as_bytes()[8..16], &[
            0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88
        ]);
    }

    #[test]
    fn missing_ids_are_synthesized_not_dropped() {
        // A span with no trace/span id still parses; ids are minted (v7).
        let json = r#"{"resourceSpans":[{"scopeSpans":[{"spans":[{"name":"x"}]}]}]}"#;
        let record = parse_one(json);
        assert_eq!(record.trace_id.0.get_version_num(), 7);
        assert_eq!(record.span_id.0.get_version_num(), 7);
    }

    #[test]
    fn unknown_span_kind_attribute_falls_back_to_workflow() {
        let attrs = r#",
            "attributes": [
                {"key": "trace_weft.span.kind", "value": {"stringValue": "Nonsense"}}
            ]"#;
        let record = parse_one(&payload(attrs));
        assert_eq!(record.span_kind, TraceWeftSpanKind::Workflow);
    }

    #[test]
    fn any_value_to_json_handles_scalar_variants() {
        let kv = |v| KeyValue {
            key: "k".into(),
            value: Some(AnyValue { value: Some(v) }),
            ..Default::default()
        };
        assert_eq!(
            any_value_to_json(&kv(AnyValueEnum::IntValue(42))),
            serde_json::json!(42)
        );
        assert_eq!(
            any_value_to_json(&kv(AnyValueEnum::BoolValue(true))),
            serde_json::json!(true)
        );
    }

    // --- Handler-level tests ---

    use axum::body::Body;
    use axum::http::Request;
    use std::sync::{Arc, Mutex};
    use tower::ServiceExt;
    use trace_weft_core::EventRecord;

    #[derive(Default)]
    struct CapturingStore {
        spans: Mutex<Vec<SpanRecord>>,
    }

    #[async_trait::async_trait]
    impl TraceStore for CapturingStore {
        async fn record_span(&self, span: SpanRecord) -> anyhow::Result<()> {
            self.spans.lock().unwrap().push(span);
            Ok(())
        }
        async fn record_event(&self, _event: EventRecord) -> anyhow::Result<()> {
            Ok(())
        }
    }

    async fn post(app: &Router, body: &str) -> StatusCode {
        app.clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/traces")
                    .header("Content-Type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap()
            .status()
    }

    #[tokio::test]
    async fn handler_ingests_valid_payload_and_preserves_ids() {
        let store = Arc::new(CapturingStore::default());
        let app = ingest_router(IngestState {
            store: store.clone(),
        });

        let status = post(&app, &payload("")).await;
        assert_eq!(status, StatusCode::OK);

        let spans = store.spans.lock().unwrap();
        assert_eq!(spans.len(), 1);
        assert_eq!(
            spans[0].trace_id.0,
            Uuid::parse_str("0123456789abcdef0011223344556677").unwrap()
        );
    }

    #[tokio::test]
    async fn handler_rejects_malformed_payload_with_400() {
        let store = Arc::new(CapturingStore::default());
        let app = ingest_router(IngestState {
            store: store.clone(),
        });

        // Not even JSON.
        assert_eq!(post(&app, "this is not json").await, StatusCode::BAD_REQUEST);
        // JSON, but the IDs are not valid hex — proto's hex decoder rejects it.
        let bad_hex = post(
            &app,
            r#"{"resourceSpans":[{"scopeSpans":[{"spans":[{"traceId":"zzzz","spanId":"zz"}]}]}]}"#,
        )
        .await;
        assert_eq!(bad_hex, StatusCode::BAD_REQUEST);

        // Nothing was recorded for either rejected request.
        assert_eq!(store.spans.lock().unwrap().len(), 0);
    }
}
