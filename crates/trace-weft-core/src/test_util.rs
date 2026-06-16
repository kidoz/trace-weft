//! Deterministic sample records for tests.
//!
//! Available to dependent crates through the `test-util` feature; not part of
//! the production API. Fixed UUIDs and timestamps keep assertions stable.

use std::collections::HashMap;

use crate::{
    BlobHash, BlobRef, CapturePolicy, CheckpointRecord, CostEstimate, EventId, EventKind,
    EventRecord, RedactionStatus, ReplayMode, RunId, SessionId, SideEffectPolicy, SpanId,
    SpanRecord, SpanStatus, TokenUsage, TraceId, TraceWeftSpanKind,
};

fn fixed_uuid(seed: u128) -> uuid::Uuid {
    uuid::Uuid::from_u128(seed)
}

/// A `BlobRef` whose hash and preview are derived from `seed`.
pub fn sample_blob_ref(seed: u8) -> BlobRef {
    BlobRef {
        hash: BlobHash(format!("sha256:{seed:064x}")),
        content_type: "text/plain".into(),
        size_bytes: 128 + seed as u64,
        created_at_timestamp: 1_715_000_000_000,
        redaction_status: RedactionStatus::Redacted,
        encryption_status: "none".into(),
        storage_backend: "local".into(),
        preview_text_redacted: Some(format!("preview-{seed}")),
    }
}

/// An LLM-call `SpanRecord` with every optional field populated.
pub fn sample_span_full() -> SpanRecord {
    let mut attributes = HashMap::new();
    attributes.insert("custom.key".to_string(), serde_json::json!("value"));
    let mut otel_attributes = HashMap::new();
    otel_attributes.insert(
        "gen_ai.operation.name".to_string(),
        serde_json::json!("chat"),
    );
    let mut openinference_attributes = HashMap::new();
    openinference_attributes.insert("llm.system".to_string(), serde_json::json!("openai"));
    let mut breakdown = HashMap::new();
    breakdown.insert("cache_read".to_string(), 12u64);

    SpanRecord {
        trace_id: TraceId(fixed_uuid(1)),
        span_id: SpanId(fixed_uuid(2)),
        parent_span_id: Some(SpanId(fixed_uuid(3))),
        run_id: RunId(fixed_uuid(4)),
        session_id: Some(SessionId(fixed_uuid(5))),
        user_id_hash: Some("user-hash".into()),
        project_id: Some("proj_demo".into()),
        span_kind: TraceWeftSpanKind::LlmCall,
        name: "draft_answer".into(),
        start_time: 1_715_000_000_000,
        end_time: Some(1_715_000_005_000),
        status: SpanStatus::Ok,
        status_message: Some("completed".into()),
        error_type: None,
        error_message_redacted: None,
        attributes,
        otel_attributes,
        openinference_attributes,
        memory_state: Some(serde_json::json!({"scratchpad": "notes"})),
        input_ref: Some(sample_blob_ref(1)),
        output_ref: Some(sample_blob_ref(2)),
        prompt_template_id: Some("support-answer".into()),
        prompt_version: Some("v7".into()),
        model_provider: Some("openai".into()),
        model_name: Some("gpt-4.1".into()),
        tool_name: Some("kb_search".into()),
        tool_schema_hash: Some("schema-hash".into()),
        retrieval_query_hash: Some("query-hash".into()),
        retrieved_document_refs: vec![sample_blob_ref(3)],
        token_usage: Some(TokenUsage {
            input: 100,
            output: 50,
            reasoning: Some(25),
            breakdown,
        }),
        cost_estimate: Some(CostEstimate {
            currency: "USD".into(),
            amount: 0.0123,
        }),
        latency_ms: Some(5000),
        retry_count: Some(1),
        cache_hit: Some(false),
        redaction_policy: CapturePolicy::RedactedPreview,
        schema_version: "1.0".into(),
    }
}

/// A tool `SpanRecord` with every optional field left empty.
pub fn sample_span_minimal() -> SpanRecord {
    SpanRecord {
        trace_id: TraceId(fixed_uuid(0x101)),
        span_id: SpanId(fixed_uuid(0x102)),
        parent_span_id: None,
        run_id: RunId(fixed_uuid(0x103)),
        session_id: None,
        user_id_hash: None,
        project_id: None,
        span_kind: TraceWeftSpanKind::Tool,
        name: "kb_search".into(),
        start_time: 1_715_000_000_000,
        end_time: None,
        status: SpanStatus::InProgress,
        status_message: None,
        error_type: None,
        error_message_redacted: None,
        attributes: HashMap::new(),
        otel_attributes: HashMap::new(),
        openinference_attributes: HashMap::new(),
        memory_state: None,
        input_ref: None,
        output_ref: None,
        prompt_template_id: None,
        prompt_version: None,
        model_provider: None,
        model_name: None,
        tool_name: None,
        tool_schema_hash: None,
        retrieval_query_hash: None,
        retrieved_document_refs: vec![],
        token_usage: None,
        cost_estimate: None,
        latency_ms: None,
        retry_count: None,
        cache_hit: None,
        redaction_policy: CapturePolicy::MetadataOnly,
        schema_version: "1.0".into(),
    }
}

/// An `EventRecord` parented to the span from [`sample_span_full`].
pub fn sample_event() -> EventRecord {
    let mut attributes = HashMap::new();
    attributes.insert("attempt".to_string(), serde_json::json!(2));
    attributes.insert("reason".to_string(), serde_json::json!("rate_limited"));

    EventRecord {
        event_id: EventId(fixed_uuid(0x20)),
        trace_id: TraceId(fixed_uuid(1)),
        run_id: RunId(fixed_uuid(4)),
        parent_span_id: Some(SpanId(fixed_uuid(2))),
        seq: 3,
        event_kind: EventKind::Retry,
        name: "llm_retry".into(),
        timestamp: 1_715_000_002_500,
        attributes,
        schema_version: "1.0".into(),
    }
}

/// An `EventRecord` with no parent and no attributes.
pub fn sample_event_minimal() -> EventRecord {
    EventRecord {
        event_id: EventId(fixed_uuid(0x21)),
        trace_id: TraceId(fixed_uuid(0x101)),
        run_id: RunId(fixed_uuid(0x103)),
        parent_span_id: None,
        seq: 0,
        event_kind: EventKind::Log,
        name: "started".into(),
        timestamp: 1_715_000_000_000,
        attributes: HashMap::new(),
        schema_version: "1.0".into(),
    }
}

/// A `CheckpointRecord` tied to the IDs used by [`sample_span_full`].
pub fn sample_checkpoint() -> CheckpointRecord {
    CheckpointRecord {
        id: fixed_uuid(10),
        trace_id: TraceId(fixed_uuid(1)),
        span_id: SpanId(fixed_uuid(2)),
        sequence: 1,
        state_hash: "state-hash".into(),
        input_hash: BlobHash("sha256:input".into()),
        output_hash: BlobHash("sha256:output".into()),
        side_effect_policy: SideEffectPolicy::ReadOnly,
        replay_mode: ReplayMode::Cached,
        created_at: 1_715_000_001_000,
    }
}
