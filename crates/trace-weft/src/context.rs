//! Ambient span context.
//!
//! A task-local "current span" so child spans link to their parent without the
//! caller threading `(TraceId, RunId, SpanId)` through every signature.
//! `SpanBuilder::run` and the instrumentation macros set this for the duration
//! of the instrumented body; spans created inside that body read it
//! automatically. `SpanBuilder::with_parent` remains the explicit override for
//! cross-task / cross-thread handoffs.

use std::future::Future;
use trace_weft_core::{RunId, SpanId, SpanRecord, TraceId};

/// The trace/run/span identity threaded down to child spans.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpanContext {
    pub trace_id: TraceId,
    pub run_id: RunId,
    pub span_id: SpanId,
}

tokio::task_local! {
    static CURRENT_SPAN: SpanContext;
}

/// The ambient span context for the current task, if one has been set.
pub fn current_span_context() -> Option<SpanContext> {
    CURRENT_SPAN.try_with(|ctx| *ctx).ok()
}

/// Run `future` with `ctx` installed as the ambient span context. Spans created
/// inside the future link to `ctx` as their parent unless they set one
/// explicitly.
pub fn scope_current<F>(ctx: SpanContext, future: F) -> impl Future<Output = F::Output>
where
    F: Future,
{
    CURRENT_SPAN.scope(ctx, future)
}

/// Link a span to the ambient parent when it has no explicit parent. A span
/// that already carries a `parent_span_id` (e.g. via `with_parent`) is left
/// untouched.
pub(crate) fn link_to_ambient(span: &mut SpanRecord) {
    if span.parent_span_id.is_some() {
        return;
    }
    if let Some(parent) = current_span_context() {
        span.trace_id = parent.trace_id;
        span.run_id = parent.run_id;
        span.parent_span_id = Some(parent.span_id);
    }
}

impl SpanContext {
    pub(crate) fn of(span: &SpanRecord) -> Self {
        Self {
            trace_id: span.trace_id,
            run_id: span.run_id,
            span_id: span.span_id,
        }
    }
}
