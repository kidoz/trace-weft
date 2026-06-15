//! Intra-span event recording.
//!
//! Events are point-in-time occurrences inside a span (a retry, a budget check,
//! a guardrail trip, an REPL step). Build one with [`event`], attach
//! attributes, and `.record()` it. Like spans, events auto-link to the ambient
//! span context set by `SpanBuilder::run` / the macros, so an event recorded
//! inside an instrumented body is parented to that span without manual IDs.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use trace_weft_core::{EventId, EventKind, EventRecord, RunId, SpanId, TraceId};
use uuid::Uuid;

/// Process-wide monotonic ordering hint for events.
static EVENT_SEQ: AtomicU64 = AtomicU64::new(0);

pub struct EventBuilder {
    event: EventRecord,
}

impl EventBuilder {
    pub fn new(kind: EventKind, name: impl Into<String>) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        let mut event = EventRecord {
            event_id: EventId(Uuid::now_v7()),
            trace_id: TraceId(Uuid::now_v7()),
            run_id: RunId(Uuid::now_v7()),
            parent_span_id: None,
            seq: 0,
            event_kind: kind,
            name: name.into(),
            timestamp: now,
            attributes: HashMap::new(),
            schema_version: "1.0".to_string(),
        };

        // Auto-link to the current span unless overridden via `with_parent`.
        if let Some(ctx) = crate::context::current_span_context() {
            event.trace_id = ctx.trace_id;
            event.run_id = ctx.run_id;
            event.parent_span_id = Some(ctx.span_id);
        }

        Self { event }
    }

    /// Explicitly parent this event, overriding any ambient context.
    pub fn with_parent(mut self, trace_id: TraceId, run_id: RunId, parent_id: SpanId) -> Self {
        self.event.trace_id = trace_id;
        self.event.run_id = run_id;
        self.event.parent_span_id = Some(parent_id);
        self
    }

    pub fn attribute(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        self.event.attributes.insert(key.into(), value);
        self
    }

    pub fn attributes(mut self, attrs: HashMap<String, serde_json::Value>) -> Self {
        self.event.attributes.extend(attrs);
        self
    }

    /// Assign the ordering `seq` and send the event to the global recorder.
    pub async fn record(mut self) {
        self.event.seq = EVENT_SEQ.fetch_add(1, Ordering::Relaxed);
        crate::record_event(self.event).await;
    }
}

/// Start building an intra-span event of `kind` named `name`.
pub fn event(kind: EventKind, name: impl Into<String>) -> EventBuilder {
    EventBuilder::new(kind, name)
}
