use opentelemetry::KeyValue;
use trace_weft_core::{SpanRecord, TraceWeftSpanKind};

/// Maps a TraceWeft SpanRecord into OpenInference semantic attributes.
pub fn map_to_openinference_attributes(record: &SpanRecord) -> Vec<KeyValue> {
    let mut attributes = Vec::new();

    let openinference_span_kind = match record.span_kind {
        TraceWeftSpanKind::LlmCall => "LLM",
        TraceWeftSpanKind::Tool => "TOOL",
        TraceWeftSpanKind::Agent => "AGENT",
        TraceWeftSpanKind::Retrieval => "RETRIEVER",
        TraceWeftSpanKind::Embedding => "EMBEDDING",
        TraceWeftSpanKind::Rerank => "RERANKER",
        TraceWeftSpanKind::Evaluator => "EVALUATOR",
        TraceWeftSpanKind::Guardrail => "GUARDRAIL",
        _ => "CHAIN", // fallback
    };

    attributes.push(KeyValue::new(
        "openinference.span.kind",
        openinference_span_kind,
    ));

    if let Some(token_usage) = &record.token_usage {
        attributes.push(KeyValue::new(
            "llm.token_count.prompt",
            token_usage.input as i64,
        ));
        attributes.push(KeyValue::new(
            "llm.token_count.completion",
            token_usage.output as i64,
        ));
        let total = token_usage.input + token_usage.output;
        attributes.push(KeyValue::new("llm.token_count.total", total as i64));
    }

    if let Some(model_name) = &record.model_name {
        attributes.push(KeyValue::new("llm.model_name", model_name.clone()));
    }

    if let Some(tool_name) = &record.tool_name {
        attributes.push(KeyValue::new("tool.name", tool_name.clone()));
    }

    // NOTE: In a complete implementation we would also map input/output strings,
    // prompts, variables, and retrieved documents into their respective
    // openinference attributes like 'input.value', 'output.value', 'retrieval.documents' etc.

    attributes
}
