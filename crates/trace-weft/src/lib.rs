pub mod builder;
pub mod context;
pub mod eval;
pub mod hitl;
pub mod replay;

pub use builder::{
    SpanBuilder, agent as build_agent, llm_call as build_llm_call, tool as build_tool,
};
pub use context::{SpanContext, current_span_context, scope_current};

pub use hitl::{HitlResponse, get_pending_approvals, register_approval, resolve_approval};
pub use replay::{ReplayConfig, init_replay};

pub use trace_weft_core::*;

pub use trace_weft_macros::{agent, llm_call, tool};
pub use trace_weft_recorder::{LocalConfig, LocalRecorder, TraceStore};

// Re-export uuid so macros can use trace_weft::uuid::Uuid
pub use uuid;

use std::sync::Arc;
use tokio::sync::OnceCell;

static RECORDER: OnceCell<Arc<dyn TraceStore>> = OnceCell::const_new();

pub async fn init_local(config: LocalConfig) -> anyhow::Result<()> {
    let recorder = LocalRecorder::new(config).await?;
    init_custom(Arc::new(recorder))
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
