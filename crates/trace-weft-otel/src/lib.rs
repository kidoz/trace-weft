use anyhow::Result;
use opentelemetry::{
    KeyValue,
    trace::{Span, SpanKind, Status, TraceError, Tracer},
};
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::{Resource, trace as sdktrace};
use trace_weft_core::{SpanRecord, SpanStatus, TraceWeftSpanKind};

pub struct OtelExporter {
    tracer: sdktrace::Tracer,
}

impl OtelExporter {
    pub fn new(endpoint: &str, service_name: &str) -> Result<Self, TraceError> {
        let tracer = opentelemetry_otlp::new_pipeline()
            .tracing()
            .with_exporter(
                opentelemetry_otlp::new_exporter()
                    .tonic()
                    .with_endpoint(endpoint),
            )
            .with_trace_config(sdktrace::config().with_resource(Resource::new(vec![
                KeyValue::new(
                    opentelemetry_semantic_conventions::resource::SERVICE_NAME,
                    service_name.to_string(),
                ),
            ])))
            .install_batch(opentelemetry_sdk::runtime::Tokio)?;

        Ok(Self { tracer })
    }

    pub fn export_span(&self, record: &SpanRecord) {
        let start_time =
            std::time::UNIX_EPOCH + std::time::Duration::from_millis(record.start_time);
        let end_time = record
            .end_time
            .map(|t| std::time::UNIX_EPOCH + std::time::Duration::from_millis(t))
            .unwrap_or_else(std::time::SystemTime::now);

        let otel_span_kind = match record.span_kind {
            TraceWeftSpanKind::LlmCall => SpanKind::Client,
            TraceWeftSpanKind::Tool => SpanKind::Internal,
            TraceWeftSpanKind::Retrieval => SpanKind::Client,
            _ => SpanKind::Internal,
        };

        let mut builder = self.tracer.span_builder(record.name.clone());
        builder.span_kind = Some(otel_span_kind);
        builder.start_time = Some(start_time);

        // TODO: Proper TraceId and SpanId propagation

        let mut attributes = Vec::new();

        // GenAI Semantic Conventions mapping
        if let Some(provider) = &record.model_provider {
            attributes.push(KeyValue::new("gen_ai.provider.name", provider.clone()));
        }
        if let Some(model) = &record.model_name {
            attributes.push(KeyValue::new("gen_ai.request.model", model.clone()));
        }
        if let Some(tool) = &record.tool_name {
            attributes.push(KeyValue::new("gen_ai.tool.name", tool.clone()));
        }

        // Custom TraceWeft attributes
        attributes.push(KeyValue::new(
            "trace_weft.span.kind",
            format!("{:?}", record.span_kind),
        ));

        if let Some(usage) = &record.token_usage {
            attributes.push(KeyValue::new(
                "gen_ai.usage.input_tokens",
                usage.input as i64,
            ));
            attributes.push(KeyValue::new(
                "gen_ai.usage.output_tokens",
                usage.output as i64,
            ));
            if let Some(reasoning) = usage.reasoning {
                attributes.push(KeyValue::new(
                    "gen_ai.usage.reasoning_tokens",
                    reasoning as i64,
                ));
            }
        }

        builder.attributes = Some(attributes);

        let mut span = builder.start(&self.tracer);

        match record.status {
            SpanStatus::Error => {
                span.set_status(Status::Error {
                    description: record
                        .error_message_redacted
                        .clone()
                        .unwrap_or_default()
                        .into(),
                });
            }
            SpanStatus::Ok => {
                span.set_status(Status::Ok);
            }
            _ => {}
        }

        span.end_with_timestamp(end_time);
    }

    pub fn shutdown(&self) {
        opentelemetry::global::shutdown_tracer_provider();
    }
}
