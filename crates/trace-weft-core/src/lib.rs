use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub mod redactor;

// --- IDs ---

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TraceId(pub uuid::Uuid);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SpanId(pub uuid::Uuid);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RunId(pub uuid::Uuid);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SessionId(pub uuid::Uuid);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct BlobHash(pub String);

// --- Enums ---

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TraceWeftSpanKind {
    Workflow,
    Agent,
    LlmCall,
    Embedding,
    Retrieval,
    Rerank,
    Tool,
    Memory,
    State,
    Planner,
    Router,
    Guardrail,
    Evaluator,
    Handoff,
    Checkpoint,
    Replay,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpanStatus {
    Ok,
    Error,
    InProgress,
    Skipped,
    Cancelled,
    PendingApproval,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapturePolicy {
    MetadataOnly,
    RedactedPreview,
    FullContentLocalOnly,
    FullContentExportable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RedactionStatus {
    Unredacted,
    Redacted,
    RedactionFailed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReplayMode {
    Cached,
    Reexecute,
    Mocked,
    Skipped,
    BlockedSideEffect,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SideEffectPolicy {
    None,
    ReadOnly,
    IdempotentWrite,
    ExternalWrite,
    PaymentOrSensitiveAction,
    Unknown,
}

// --- Structs ---

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TokenUsage {
    pub input: u64,
    pub output: u64,
    pub reasoning: Option<u64>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub breakdown: HashMap<String, u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CostEstimate {
    pub currency: String,
    pub amount: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BlobRef {
    pub hash: BlobHash,
    pub content_type: String,
    pub size_bytes: u64,
    pub created_at_timestamp: u64, // Unix timestamp in ms
    pub redaction_status: RedactionStatus,
    pub encryption_status: String,
    pub storage_backend: String,
    pub preview_text_redacted: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpanRecord {
    pub trace_id: TraceId,
    pub span_id: SpanId,
    pub parent_span_id: Option<SpanId>,
    pub run_id: RunId,
    pub session_id: Option<SessionId>,
    pub user_id_hash: Option<String>,
    pub span_kind: TraceWeftSpanKind,
    pub name: String,
    pub start_time: u64,       // ms timestamp
    pub end_time: Option<u64>, // ms timestamp
    pub status: SpanStatus,
    pub status_message: Option<String>,
    pub error_type: Option<String>,
    pub error_message_redacted: Option<String>,

    // Core Attributes
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub attributes: HashMap<String, serde_json::Value>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub otel_attributes: HashMap<String, serde_json::Value>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub openinference_attributes: HashMap<String, serde_json::Value>,

    pub memory_state: Option<serde_json::Value>,

    // LLM / Tool specific
    pub input_ref: Option<BlobRef>,
    pub output_ref: Option<BlobRef>,
    pub prompt_template_id: Option<String>,
    pub prompt_version: Option<String>,
    pub model_provider: Option<String>,
    pub model_name: Option<String>,
    pub tool_name: Option<String>,
    pub tool_schema_hash: Option<String>,

    // Retrieval specific
    pub retrieval_query_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub retrieved_document_refs: Vec<BlobRef>,

    // Usage/Performance
    pub token_usage: Option<TokenUsage>,
    pub cost_estimate: Option<CostEstimate>,
    pub latency_ms: Option<u64>,
    pub retry_count: Option<u32>,
    pub cache_hit: Option<bool>,

    // Metadata
    pub redaction_policy: CapturePolicy,
    pub schema_version: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TraceRecord {
    pub trace_id: TraceId,
    pub run_id: RunId,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub spans: Vec<SpanRecord>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CheckpointRecord {
    pub id: uuid::Uuid,
    pub trace_id: TraceId,
    pub span_id: SpanId,
    pub sequence: u64,
    pub state_hash: String,
    pub input_hash: BlobHash,
    pub output_hash: BlobHash,
    pub side_effect_policy: SideEffectPolicy,
    pub replay_mode: ReplayMode,
    pub created_at: u64,
}

// --- Traits ---

pub struct RedactionResult {
    pub redacted_text: String,
    pub status: RedactionStatus,
}

pub trait Redactor: Send + Sync {
    fn redact(&self, input: &str) -> RedactionResult;
}

#[async_trait::async_trait]
pub trait BlobStore: Send + Sync {
    async fn put_blob(
        &self,
        hash: &BlobHash,
        content_type: &str,
        content: &[u8],
    ) -> anyhow::Result<()>;
    async fn get_blob(&self, hash: &BlobHash) -> anyhow::Result<Option<Vec<u8>>>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn test_json_roundtrip() {
        let span = SpanRecord {
            trace_id: TraceId(Uuid::now_v7()),
            span_id: SpanId(Uuid::now_v7()),
            parent_span_id: None,
            run_id: RunId(Uuid::now_v7()),
            session_id: None,
            user_id_hash: None,
            span_kind: TraceWeftSpanKind::LlmCall,
            name: "draft_answer".into(),
            start_time: 1715000000000,
            end_time: Some(1715000005000),
            status: SpanStatus::Ok,
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
            prompt_version: Some("v1".into()),
            model_provider: Some("openai".into()),
            model_name: Some("gpt-4-turbo".into()),
            tool_name: None,
            tool_schema_hash: None,
            retrieval_query_hash: None,
            retrieved_document_refs: vec![],
            token_usage: Some(TokenUsage {
                input: 100,
                output: 50,
                reasoning: None,
                breakdown: HashMap::new(),
            }),
            cost_estimate: None,
            latency_ms: Some(5000),
            retry_count: None,
            cache_hit: Some(false),
            redaction_policy: CapturePolicy::RedactedPreview,
            schema_version: "1.0".into(),
        };

        let json = serde_json::to_string(&span).unwrap();
        let parsed: SpanRecord = serde_json::from_str(&json).unwrap();

        assert_eq!(span, parsed);
    }
}
