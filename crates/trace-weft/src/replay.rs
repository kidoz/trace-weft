use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;

lazy_static::lazy_static! {
    static ref REPLAY_CONTEXT: Mutex<Option<ReplayConfig>> = Mutex::new(None);
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ReplayConfig {
    pub mocked_spans: HashMap<String, Value>,
    pub block_side_effects: bool,
}

impl ReplayConfig {
    pub fn load_from_env() -> Option<Self> {
        if let Ok(path_str) = std::env::var("TRACE_WEFT_REPLAY_FILE") {
            let path = Path::new(&path_str);
            if path.exists()
                && let Ok(content) = std::fs::read_to_string(path)
            {
                if let Ok(config) = serde_json::from_str::<Self>(&content) {
                    tracing::info!("Loaded TraceWeft replay config from {}", path_str);
                    return Some(config);
                } else {
                    tracing::error!("Failed to parse TraceWeft replay config at {}", path_str);
                }
            }
        }
        None
    }
}

pub fn init_replay(config: ReplayConfig) {
    if let Ok(mut ctx) = REPLAY_CONTEXT.lock() {
        *ctx = Some(config);
    }
}

pub fn get_mocked_output(span_name: &str) -> Option<Value> {
    if let Ok(ctx) = REPLAY_CONTEXT.lock()
        && let Some(config) = ctx.as_ref()
    {
        return config.mocked_spans.get(span_name).cloned();
    }
    None
}
