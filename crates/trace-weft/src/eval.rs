use std::sync::{Arc, Mutex};
use trace_weft_core::{SpanRecord, TraceWeftSpanKind};
use trace_weft_recorder::TraceStore;

/// In-memory trace store designed specifically for capturing spans during local unit tests
/// and evaluation pipelines.
#[derive(Clone, Default)]
pub struct MemoryStore {
    pub spans: Arc<Mutex<Vec<SpanRecord>>>,
}

impl MemoryStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get_trajectory(&self) -> TraceTrajectory {
        let spans = self.spans.lock().unwrap().clone();
        TraceTrajectory { spans }
    }

    pub fn clear(&self) {
        self.spans.lock().unwrap().clear();
    }
}

#[async_trait::async_trait]
impl TraceStore for MemoryStore {
    async fn record_span(&self, span: SpanRecord) -> anyhow::Result<()> {
        self.spans.lock().unwrap().push(span);
        Ok(())
    }
}

/// A wrapper around a collection of spans to facilitate easy trajectory assertions.
pub struct TraceTrajectory {
    pub spans: Vec<SpanRecord>,
}

impl TraceTrajectory {
    /// Checks if a specific tool was called during the trace.
    pub fn contains_tool_call(&self, tool_name: &str) -> bool {
        self.spans.iter().any(|s| {
            s.span_kind == TraceWeftSpanKind::Tool && s.name == tool_name
        })
    }

    /// Checks if an error span was recorded.
    pub fn has_errors(&self) -> bool {
        self.spans.iter().any(|s| {
            s.status == trace_weft_core::SpanStatus::Error || s.span_kind == TraceWeftSpanKind::Error
        })
    }

    /// Calculates the total cost estimate of all spans in the trajectory.
    pub fn total_cost(&self) -> f64 {
        self.spans
            .iter()
            .filter_map(|s| s.cost_estimate.as_ref())
            .map(|c| c.amount)
            .sum()
    }

    /// Returns the latency of the root workflow/agent span.
    pub fn total_latency_ms(&self) -> u64 {
        self.spans
            .iter()
            .filter(|s| s.parent_span_id.is_none())
            .map(|s| s.latency_ms.unwrap_or(0))
            .sum()
    }
    
    /// Returns the total number of input tokens consumed across all LLM calls.
    pub fn total_input_tokens(&self) -> u64 {
        self.spans
            .iter()
            .filter_map(|s| s.token_usage.as_ref())
            .map(|u| u.input)
            .sum()
    }
}
