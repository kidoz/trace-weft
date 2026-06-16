use axum::{Json, Router, extract::State, http::StatusCode, response::IntoResponse, routing::post};
use std::collections::HashMap;
use trace_weft_core::{
    CapturePolicy, RunId, SpanId, SpanRecord, SpanStatus, TraceId, TraceWeftSpanKind,
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

/// A very basic and naive adapter for OTLP HTTP JSON ingestion.
/// In a real implementation, this would use `opentelemetry-proto` generated types.
async fn handle_otlp_json_traces(
    State(state): State<IngestState>,
    Json(payload): Json<serde_json::Value>,
) -> Result<impl IntoResponse, StatusCode> {
    // Naively extract resource spans
    let resource_spans = payload.get("resourceSpans").and_then(|v| v.as_array());

    if let Some(resource_spans) = resource_spans {
        for rs in resource_spans {
            let scope_spans = rs.get("scopeSpans").and_then(|v| v.as_array());
            if let Some(scope_spans) = scope_spans {
                for ss in scope_spans {
                    let spans = ss.get("spans").and_then(|v| v.as_array());
                    if let Some(spans) = spans {
                        for span_json in spans {
                            // Convert OTel span to TraceWeft SpanRecord
                            let record = map_otel_to_traceweft(span_json);
                            if let Some(record) = record {
                                let _ = state.store.record_span(record).await;
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(StatusCode::OK)
}

fn map_otel_to_traceweft(span: &serde_json::Value) -> Option<SpanRecord> {
    // This is a minimal adapter. We extract trace_id, span_id, name, etc.
    let trace_id_str = span
        .get("traceId")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    let span_id_str = span
        .get("spanId")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    let name = span
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");
    let start_time_unix_nano = span
        .get("startTimeUnixNano")
        .and_then(|v| v.as_str())
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or_default();
    let end_time_unix_nano = span
        .get("endTimeUnixNano")
        .and_then(|v| v.as_str())
        .and_then(|v| v.parse::<u64>().ok());

    // Map OTel kinds and attributes to TraceWeft GenAI attributes
    let mut otel_attributes = HashMap::new();
    let mut model_provider = None;
    let mut model_name = None;
    let mut span_kind = TraceWeftSpanKind::Workflow;

    if let Some(attrs) = span.get("attributes").and_then(|v| v.as_array()) {
        for attr in attrs {
            if let (Some(key), Some(value)) =
                (attr.get("key").and_then(|v| v.as_str()), attr.get("value"))
            {
                otel_attributes.insert(key.to_string(), value.clone());

                match key {
                    "gen_ai.provider.name" => {
                        model_provider = value
                            .get("stringValue")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string())
                    }
                    "gen_ai.request.model" => {
                        model_name = value
                            .get("stringValue")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string())
                    }
                    "trace_weft.span.kind"
                        if value
                            .get("stringValue")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            == "LlmCall" =>
                    {
                        span_kind = TraceWeftSpanKind::LlmCall;
                    }
                    _ => {}
                }
            }
        }
    }

    Some(SpanRecord {
        // Fallback to new UUIDs if parse fails for simplicity in this adapter
        trace_id: TraceId(uuid::Uuid::try_parse(trace_id_str).unwrap_or_else(|_| Uuid::now_v7())),
        span_id: SpanId(uuid::Uuid::try_parse(span_id_str).unwrap_or_else(|_| Uuid::now_v7())),
        parent_span_id: None,
        run_id: RunId(Uuid::now_v7()), // Grouping under a single run for ingested
        session_id: None,
        user_id_hash: None,
        project_id: None,
        span_kind,
        name: name.to_string(),
        start_time: start_time_unix_nano / 1_000_000,
        end_time: end_time_unix_nano.map(|t| t / 1_000_000),
        status: SpanStatus::Ok,
        status_message: None,
        error_type: None,
        error_message_redacted: None,
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
        tool_name: None,
        tool_schema_hash: None,
        retrieval_query_hash: None,
        retrieved_document_refs: vec![],
        token_usage: None,
        cost_estimate: None,
        latency_ms: end_time_unix_nano.map(|e| (e - start_time_unix_nano) / 1_000_000),
        retry_count: None,
        cache_hit: None,
        redaction_policy: CapturePolicy::MetadataOnly,
        schema_version: "1.0".to_string(),
    })
}
