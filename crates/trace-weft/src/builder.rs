use std::collections::HashMap;
use trace_weft_core::{
    BlobRef, CapturePolicy, CostEstimate, RunId, SpanId, SpanRecord, SpanStatus, TokenUsage,
    TraceId, TraceWeftSpanKind,
};
use uuid::Uuid;

pub struct SpanBuilder {
    pub span: SpanRecord,
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

    pub fn input_ref(mut self, blob_ref: BlobRef) -> Self {
        self.span.input_ref = Some(blob_ref);
        self
    }

    pub fn output_ref(mut self, blob_ref: BlobRef) -> Self {
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

    pub async fn run<F, Fut, T, E>(self, f: F) -> Result<T, E>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<T, E>>,
        E: std::fmt::Debug + std::fmt::Display + 'static,
        T: serde::de::DeserializeOwned,
    {
        let mut span = self.span;

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

        let result = f().await;
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
                span.error_type = Some(format!("{:?}", e)); // Naive type extraction
                span.error_message_redacted = Some(e.to_string()); // Naive redaction
            }
        }

        crate::record_span(span).await;

        result
    }
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
