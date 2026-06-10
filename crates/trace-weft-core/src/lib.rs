use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub mod redactor;

#[cfg(any(test, feature = "test-util"))]
pub mod test_util;

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
    use super::test_util::{
        sample_blob_ref, sample_checkpoint, sample_span_full, sample_span_minimal,
    };
    use super::*;
    use serde_json::json;

    fn roundtrip<T>(value: &T) -> T
    where
        T: serde::Serialize + serde::de::DeserializeOwned,
    {
        let json = serde_json::to_string(value).unwrap();
        serde_json::from_str(&json).unwrap()
    }

    #[test]
    fn span_record_full_roundtrip() {
        let span = sample_span_full();
        assert_eq!(roundtrip(&span), span);
    }

    #[test]
    fn span_record_minimal_roundtrip() {
        let span = sample_span_minimal();
        assert_eq!(roundtrip(&span), span);
    }

    #[test]
    fn trace_record_roundtrip() {
        let trace = TraceRecord {
            trace_id: sample_span_full().trace_id,
            run_id: sample_span_full().run_id,
            spans: vec![sample_span_full(), sample_span_minimal()],
        };
        assert_eq!(roundtrip(&trace), trace);
    }

    #[test]
    fn checkpoint_record_roundtrip() {
        let checkpoint = sample_checkpoint();
        assert_eq!(roundtrip(&checkpoint), checkpoint);
    }

    #[test]
    fn blob_ref_roundtrip() {
        let blob = sample_blob_ref(7);
        assert_eq!(roundtrip(&blob), blob);
    }

    #[test]
    fn ids_serialize_as_plain_uuid_strings() {
        let id = TraceId(uuid::Uuid::from_u128(1));
        assert_eq!(
            serde_json::to_value(id).unwrap(),
            json!("00000000-0000-0000-0000-000000000001")
        );
    }

    #[test]
    fn blob_hash_serializes_as_plain_string() {
        let hash = BlobHash("sha256:abc".into());
        assert_eq!(serde_json::to_value(&hash).unwrap(), json!("sha256:abc"));
    }

    #[test]
    fn enums_use_snake_case_wire_format() {
        assert_eq!(
            serde_json::to_value(TraceWeftSpanKind::LlmCall).unwrap(),
            json!("llm_call")
        );
        assert_eq!(
            serde_json::to_value(SpanStatus::PendingApproval).unwrap(),
            json!("pending_approval")
        );
        assert_eq!(
            serde_json::to_value(CapturePolicy::FullContentLocalOnly).unwrap(),
            json!("full_content_local_only")
        );
        assert_eq!(
            serde_json::to_value(RedactionStatus::RedactionFailed).unwrap(),
            json!("redaction_failed")
        );
        assert_eq!(
            serde_json::to_value(ReplayMode::BlockedSideEffect).unwrap(),
            json!("blocked_side_effect")
        );
        assert_eq!(
            serde_json::to_value(SideEffectPolicy::PaymentOrSensitiveAction).unwrap(),
            json!("payment_or_sensitive_action")
        );
    }

    #[test]
    fn all_span_kinds_roundtrip() {
        use TraceWeftSpanKind::*;
        for kind in [
            Workflow, Agent, LlmCall, Embedding, Retrieval, Rerank, Tool, Memory, State, Planner,
            Router, Guardrail, Evaluator, Handoff, Checkpoint, Replay, Error,
        ] {
            assert_eq!(roundtrip(&kind), kind);
        }
    }

    #[test]
    fn empty_collections_are_omitted_from_json() {
        let value = serde_json::to_value(sample_span_minimal()).unwrap();
        let object = value.as_object().unwrap();
        for key in [
            "attributes",
            "otel_attributes",
            "openinference_attributes",
            "retrieved_document_refs",
        ] {
            assert!(
                !object.contains_key(key),
                "{key} should be omitted when empty"
            );
        }
    }

    #[test]
    fn token_usage_empty_breakdown_is_omitted() {
        let usage = TokenUsage {
            input: 1,
            output: 2,
            reasoning: None,
            breakdown: HashMap::new(),
        };
        let value = serde_json::to_value(&usage).unwrap();
        assert!(!value.as_object().unwrap().contains_key("breakdown"));
        assert_eq!(roundtrip(&usage), usage);
    }

    #[test]
    fn span_record_deserializes_from_minimal_wire_payload() {
        // A producer that omits every optional field must still parse.
        let payload = json!({
            "trace_id": "00000000-0000-0000-0000-000000000001",
            "span_id": "00000000-0000-0000-0000-000000000002",
            "run_id": "00000000-0000-0000-0000-000000000004",
            "span_kind": "tool",
            "name": "kb_search",
            "start_time": 1_715_000_000_000u64,
            "status": "in_progress",
            "redaction_policy": "metadata_only",
            "schema_version": "1.0"
        });
        let parsed: SpanRecord = serde_json::from_value(payload).unwrap();
        assert_eq!(parsed.span_kind, TraceWeftSpanKind::Tool);
        assert_eq!(parsed.status, SpanStatus::InProgress);
        assert!(parsed.parent_span_id.is_none());
        assert!(parsed.attributes.is_empty());
        assert!(parsed.retrieved_document_refs.is_empty());
        assert!(parsed.token_usage.is_none());
    }
}
