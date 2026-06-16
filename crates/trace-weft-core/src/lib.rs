use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub mod redactor;

/// Shared OpenTelemetry GenAI semantic-convention attribute keys.
///
/// Defined once so the exporter (`trace-weft-otel`) and the OTLP ingest adapter
/// (`trace-weft-ingest`) agree on attribute names instead of each repeating the
/// string literals.
pub mod semconv {
    pub const GEN_AI_PROVIDER_NAME: &str = "gen_ai.provider.name";
    pub const GEN_AI_REQUEST_MODEL: &str = "gen_ai.request.model";
    pub const GEN_AI_TOOL_NAME: &str = "gen_ai.tool.name";
    pub const GEN_AI_USAGE_INPUT_TOKENS: &str = "gen_ai.usage.input_tokens";
    pub const GEN_AI_USAGE_OUTPUT_TOKENS: &str = "gen_ai.usage.output_tokens";
    pub const GEN_AI_USAGE_REASONING_TOKENS: &str = "gen_ai.usage.reasoning_tokens";

    /// TraceWeft span kind, serialized as the Rust variant name (e.g. `LlmCall`).
    pub const TRACE_WEFT_SPAN_KIND: &str = "trace_weft.span.kind";
}

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct EventId(pub uuid::Uuid);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct BlobHash(pub String);

/// Generate `new()` (fresh UUIDv7, time-ordered) and `Default` for a UUID
/// newtype, so integrators don't have to depend on `uuid` directly.
macro_rules! uuid_id {
    ($($t:ident),+ $(,)?) => {
        $(
            impl $t {
                /// Create a fresh, time-ordered identifier.
                pub fn new() -> Self {
                    Self(uuid::Uuid::now_v7())
                }
            }
            impl Default for $t {
                fn default() -> Self {
                    Self::new()
                }
            }
        )+
    };
}

uuid_id!(TraceId, SpanId, RunId, SessionId, EventId);

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

/// Kind of an intra-span event — a point-in-time occurrence within a span's
/// lifetime (a retry, a budget check, a guardrail trip, an REPL step), as
/// opposed to a span, which has a duration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    LlmCall,
    ToolCall,
    ReplExec,
    Rpc,
    Budget,
    Guardrail,
    Retry,
    Termination,
    Log,
    Custom,
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
    /// Tenant the span belongs to. Set server-side from the authenticated API
    /// key at ingest (clients cannot assert it); `None` for local-first
    /// single-tenant recording. Used to scope trace queries per project.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
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

/// A point-in-time event recorded within a span. Events carry their parent
/// span and an ordering `seq` so an event stream (retries, budget checks,
/// guardrail trips, REPL steps) survives without collapsing into many tiny
/// spans.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EventRecord {
    pub event_id: EventId,
    pub trace_id: TraceId,
    pub run_id: RunId,
    pub parent_span_id: Option<SpanId>,
    /// Monotonic ordering hint within the process/trace.
    pub seq: u64,
    pub event_kind: EventKind,
    pub name: String,
    pub timestamp: u64, // ms
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub attributes: HashMap<String, serde_json::Value>,
    pub schema_version: String,
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
        sample_blob_ref, sample_checkpoint, sample_event, sample_event_minimal, sample_span_full,
        sample_span_minimal,
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
    fn event_record_roundtrip() {
        let event = sample_event();
        assert_eq!(roundtrip(&event), event);
    }

    #[test]
    fn event_record_minimal_roundtrip() {
        let event = sample_event_minimal();
        assert_eq!(roundtrip(&event), event);
        // Empty attributes are omitted from the wire format.
        let value = serde_json::to_value(&event).unwrap();
        assert!(!value.as_object().unwrap().contains_key("attributes"));
    }

    #[test]
    fn event_kind_uses_snake_case_wire_format() {
        assert_eq!(
            serde_json::to_value(EventKind::LlmCall).unwrap(),
            json!("llm_call")
        );
        assert_eq!(
            serde_json::to_value(EventKind::ReplExec).unwrap(),
            json!("repl_exec")
        );
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
    fn id_constructors_make_distinct_v7_uuids() {
        let a = TraceId::new();
        let b = TraceId::new();
        assert_ne!(a, b);
        assert_eq!(a.0.get_version_num(), 7);
        // Default delegates to new().
        assert_ne!(SpanId::default(), SpanId::default());
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
