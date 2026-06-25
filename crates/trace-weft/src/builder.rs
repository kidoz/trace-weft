use serde::Serialize;
use std::collections::HashMap;
use trace_weft_core::{
    BlobRef, CapturePolicy, CostEstimate, RunId, SpanId, SpanRecord, SpanStatus, TokenUsage,
    TraceId, TraceWeftSpanKind,
};
use uuid::Uuid;

pub struct SpanBuilder {
    pub span: SpanRecord,
    pending_input_ref: Option<PendingCapture>,
    pending_output_ref: Option<PendingCapture>,
}

struct PendingCapture {
    label: String,
    value: serde_json::Value,
}

impl SpanBuilder {
    pub fn new(kind: TraceWeftSpanKind, name: impl Into<String>) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        Self {
            span: SpanRecord {
                trace_id: TraceId(Uuid::now_v7()),
                span_id: SpanId(Uuid::now_v7()),
                parent_span_id: None,
                run_id: RunId(Uuid::now_v7()),
                session_id: None,
                user_id_hash: None,
                project_id: None,
                span_kind: kind,
                name: name.into(),
                start_time: now,
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
                schema_version: "1.0".to_string(),
            },
            pending_input_ref: None,
            pending_output_ref: None,
        }
    }

    pub fn provider(mut self, provider: impl Into<String>) -> Self {
        self.span.model_provider = Some(provider.into());
        self
    }

    pub fn model(mut self, model: impl Into<String>) -> Self {
        self.span.model_name = Some(model.into());
        self
    }

    pub fn prompt_version(mut self, version: impl Into<String>) -> Self {
        self.span.prompt_version = Some(version.into());
        self
    }

    pub fn tool_name(mut self, tool: impl Into<String>) -> Self {
        self.span.tool_name = Some(tool.into());
        self
    }

    /// Capture a serializable input value under `label` when the span runs.
    ///
    /// The value is converted to JSON immediately, but the blob is persisted
    /// inside [`run`](Self::run) / [`run_infallible`](Self::run_infallible) so
    /// the async blob store can be used without making every setter async.
    /// When the active capture policy is `MetadataOnly`, no blob is written.
    pub fn input_ref<T: Serialize>(mut self, label: impl Into<String>, value: &T) -> Self {
        self.pending_input_ref = Some(PendingCapture {
            label: label.into(),
            value: serde_json::to_value(value).unwrap_or(serde_json::Value::Null),
        });
        self
    }

    /// Capture a serializable output value under `label` when the span runs.
    ///
    /// This is for callers that already have or can cheaply precompute an
    /// output-like value. Use macros when you want successful function returns
    /// captured automatically.
    pub fn output_ref<T: Serialize>(mut self, label: impl Into<String>, value: &T) -> Self {
        self.pending_output_ref = Some(PendingCapture {
            label: label.into(),
            value: serde_json::to_value(value).unwrap_or(serde_json::Value::Null),
        });
        self
    }

    /// Attach a pre-existing input blob reference without writing new content.
    pub fn input_blob_ref(mut self, blob_ref: BlobRef) -> Self {
        self.span.input_ref = Some(blob_ref);
        self
    }

    /// Attach a pre-existing output blob reference without writing new content.
    pub fn output_blob_ref(mut self, blob_ref: BlobRef) -> Self {
        self.span.output_ref = Some(blob_ref);
        self
    }

    pub fn token_usage(mut self, usage: TokenUsage) -> Self {
        self.span.token_usage = Some(usage);
        self
    }

    pub fn cost(mut self, cost: CostEstimate) -> Self {
        self.span.cost_estimate = Some(cost);
        self
    }

    pub fn cache_hit(mut self, hit: bool) -> Self {
        self.span.cache_hit = Some(hit);
        self
    }

    /// Record a retrieval query hash and the documents it returned.
    pub fn retrieval(mut self, query_hash: impl Into<String>, doc_refs: Vec<BlobRef>) -> Self {
        self.span.retrieval_query_hash = Some(query_hash.into());
        self.span.retrieved_document_refs = doc_refs;
        self
    }

    /// Insert a single free-form attribute.
    pub fn attribute(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        self.span.attributes.insert(key.into(), value);
        self
    }

    /// Merge a map of free-form attributes into the span.
    pub fn attributes(mut self, attrs: HashMap<String, serde_json::Value>) -> Self {
        self.span.attributes.extend(attrs);
        self
    }

    pub fn with_parent(mut self, trace_id: TraceId, run_id: RunId, parent_id: SpanId) -> Self {
        self.span.trace_id = trace_id;
        self.span.run_id = run_id;
        self.span.parent_span_id = Some(parent_id);
        self
    }

    pub async fn wait_for_approval(mut self) -> Result<crate::hitl::HitlResponse, String> {
        crate::context::link_to_ambient(&mut self.span);
        self.span.redaction_policy = crate::capture_policy();
        self.capture_pending_refs().await;
        let span_id = self.span.span_id.0.to_string();
        self.span.status = SpanStatus::PendingApproval;

        let rx = crate::hitl::register_approval(span_id);

        // Record the span so the UI sees it is pending
        crate::record_span(self.span.clone()).await;

        // Wait for the approval/rejection from the UI/server
        match rx.await {
            Ok(response) => {
                // we should end the span? actually this is a breakpoint.
                // a breakpoint span is its own span. So we can just mark it done.
                self.span.end_time = Some(
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_millis() as u64,
                );
                self.span.latency_ms = Some(self.span.end_time.unwrap() - self.span.start_time);
                self.span.status = SpanStatus::Ok;
                crate::record_span(self.span).await;
                Ok(response)
            }
            Err(_) => Err("Hitl approval channel closed unexpectedly".to_string()),
        }
    }

    pub async fn run<F, Fut, T, E>(mut self, f: F) -> Result<T, E>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<T, E>>,
        E: std::fmt::Debug + std::fmt::Display + 'static,
        T: serde::de::DeserializeOwned,
    {
        self.span.redaction_policy = crate::capture_policy();
        self.capture_pending_refs().await;
        let mut span = self.span;
        crate::context::link_to_ambient(&mut span);

        // Mock / Replay interception
        if let Some(mocked) = crate::replay::get_mocked_output(&span.name) {
            span.end_time = Some(span.start_time);
            span.latency_ms = Some(0);
            span.status = SpanStatus::Ok;
            span.attributes
                .insert("replayed".to_string(), serde_json::json!(true));
            crate::record_span(span.clone()).await;

            if let Ok(value) = serde_json::from_value::<T>(mocked) {
                return Ok(value);
            }
        }

        // Install this span as the ambient parent for spans created inside `f`.
        let ctx = crate::context::SpanContext::of(&span);
        let result = crate::context::scope_current(ctx, f()).await;
        span.end_time = Some(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
        );
        span.latency_ms = Some(span.end_time.unwrap() - span.start_time);

        match &result {
            Ok(_) => {
                span.status = SpanStatus::Ok;
            }
            Err(e) => {
                span.status = SpanStatus::Error;
                span.error_type = Some(std::any::type_name::<E>().to_string());
                span.error_message_redacted =
                    Some(crate::redact_text(&e.to_string()).redacted_text);
            }
        }

        crate::record_span(span).await;

        result
    }

    /// Like [`run`](Self::run) but for closures that don't return `Result`. The
    /// span always completes with `Ok` status. Replay mocking (which is keyed on
    /// deserializing a mocked value) applies only to `run`, not here.
    pub async fn run_infallible<F, Fut, T>(mut self, f: F) -> T
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = T>,
    {
        self.span.redaction_policy = crate::capture_policy();
        self.capture_pending_refs().await;
        let mut span = self.span;
        crate::context::link_to_ambient(&mut span);

        let ctx = crate::context::SpanContext::of(&span);
        let result = crate::context::scope_current(ctx, f()).await;

        span.end_time = Some(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
        );
        span.latency_ms = Some(span.end_time.unwrap() - span.start_time);
        span.status = SpanStatus::Ok;
        crate::record_span(span).await;

        result
    }

    async fn capture_pending_refs(&mut self) {
        if self.span.input_ref.is_none()
            && let Some(pending) = self.pending_input_ref.take()
        {
            self.span.input_ref = capture_labeled_json(pending).await;
        }
        if self.span.output_ref.is_none()
            && let Some(pending) = self.pending_output_ref.take()
        {
            self.span.output_ref = capture_labeled_json(pending).await;
        }
    }
}

async fn capture_labeled_json(pending: PendingCapture) -> Option<BlobRef> {
    let mut object = serde_json::Map::new();
    object.insert(pending.label, pending.value);
    crate::capture_json("application/json", serde_json::Value::Object(object)).await
}

pub fn llm_call(name: impl Into<String>) -> SpanBuilder {
    SpanBuilder::new(TraceWeftSpanKind::LlmCall, name)
}

pub fn tool(name: impl Into<String>) -> SpanBuilder {
    SpanBuilder::new(TraceWeftSpanKind::Tool, name)
}

pub fn agent(name: impl Into<String>) -> SpanBuilder {
    SpanBuilder::new(TraceWeftSpanKind::Agent, name)
}
