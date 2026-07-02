pub mod builder;
pub mod capture;
pub mod context;
pub mod eval;
pub mod events;
pub mod hitl;
pub mod replay;

pub use builder::{
    SpanBuilder, SpanHandle, agent as build_agent, llm_call as build_llm_call, tool as build_tool,
};
pub use capture::{
    CaptureConfig, capture_enabled, capture_json, capture_policy, init_capture, redact_text,
};
pub use context::{SpanContext, current_span_context, scope_current};
pub use events::{EventBuilder, event};

pub use hitl::{HitlResponse, get_pending_approvals, register_approval, resolve_approval};
pub use replay::{ReplayConfig, init_replay};

pub use trace_weft_core::*;

pub use trace_weft_macros::{agent, llm_call, tool};
pub use trace_weft_recorder::{LocalConfig, LocalRecorder, NullStore, TraceStore};

// Re-export uuid and serde_json so the macros can reference them through the
// facade without the consumer depending on either crate directly.
pub use serde_json;
pub use uuid;

use std::sync::Arc;
use tokio::sync::OnceCell;

static RECORDER: OnceCell<Arc<dyn TraceStore>> = OnceCell::const_new();

pub async fn init_local(config: LocalConfig) -> anyhow::Result<()> {
    let policy = config.capture_content;
    let blob_dir = config.blob_dir.clone();

    let recorder = LocalRecorder::new(config).await?;
    init_custom(Arc::new(recorder))?;

    init_capture(CaptureConfig {
        policy,
        blobs: Arc::new(capture::FsBlobStore::new(blob_dir)),
        redactor: Arc::new(trace_weft_core::redactor::RegexRedactor::default()),
        storage_backend: "local_fs".to_string(),
    })
}

pub fn init_custom(store: Arc<dyn TraceStore>) -> anyhow::Result<()> {
    RECORDER
        .set(store)
        .map_err(|_| anyhow::anyhow!("Already initialized"))?;

    // Auto-load replay config from environment if present
    if let Some(replay_config) = ReplayConfig::load_from_env() {
        init_replay(replay_config);
    }

    Ok(())
}

pub async fn record_span(span: SpanRecord) {
    if let Some(recorder) = RECORDER.get() {
        let _ = recorder.record_span(span).await;
    }
}

pub async fn record_event(event: EventRecord) {
    if let Some(recorder) = RECORDER.get() {
        let _ = recorder.record_event(event).await;
    }
}
