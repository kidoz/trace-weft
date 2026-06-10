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

#[cfg(test)]
mod tests {
    use super::*;
    use opentelemetry::Value;
    use trace_weft_core::test_util::{sample_span_full, sample_span_minimal};

    fn attr_value<'a>(attrs: &'a [KeyValue], key: &str) -> Option<&'a Value> {
        attrs
            .iter()
            .find(|kv| kv.key.as_str() == key)
            .map(|kv| &kv.value)
    }

    #[test]
    fn maps_span_kinds_to_openinference_names() {
        let cases = [
            (TraceWeftSpanKind::LlmCall, "LLM"),
            (TraceWeftSpanKind::Tool, "TOOL"),
            (TraceWeftSpanKind::Agent, "AGENT"),
            (TraceWeftSpanKind::Retrieval, "RETRIEVER"),
            (TraceWeftSpanKind::Embedding, "EMBEDDING"),
            (TraceWeftSpanKind::Rerank, "RERANKER"),
            (TraceWeftSpanKind::Evaluator, "EVALUATOR"),
            (TraceWeftSpanKind::Guardrail, "GUARDRAIL"),
            // Kinds without a direct OpenInference equivalent fall back to CHAIN.
            (TraceWeftSpanKind::Workflow, "CHAIN"),
            (TraceWeftSpanKind::Planner, "CHAIN"),
        ];
        for (kind, expected) in cases {
            let mut span = sample_span_minimal();
            span.span_kind = kind;
            let attrs = map_to_openinference_attributes(&span);
            assert_eq!(
                attr_value(&attrs, "openinference.span.kind"),
                Some(&Value::from(expected)),
                "kind {kind:?} should map to {expected}"
            );
        }
    }

    #[test]
    fn maps_token_counts_including_total() {
        let attrs = map_to_openinference_attributes(&sample_span_full());
        assert_eq!(
            attr_value(&attrs, "llm.token_count.prompt"),
            Some(&Value::I64(100))
        );
        assert_eq!(
            attr_value(&attrs, "llm.token_count.completion"),
            Some(&Value::I64(50))
        );
        assert_eq!(
            attr_value(&attrs, "llm.token_count.total"),
            Some(&Value::I64(150))
        );
    }

    #[test]
    fn maps_model_and_tool_names() {
        let attrs = map_to_openinference_attributes(&sample_span_full());
        assert_eq!(
            attr_value(&attrs, "llm.model_name"),
            Some(&Value::from("gpt-4.1"))
        );
        assert_eq!(
            attr_value(&attrs, "tool.name"),
            Some(&Value::from("kb_search"))
        );
    }

    #[test]
    fn omits_llm_attributes_for_bare_spans() {
        let attrs = map_to_openinference_attributes(&sample_span_minimal());
        assert!(attr_value(&attrs, "llm.token_count.prompt").is_none());
        assert!(attr_value(&attrs, "llm.model_name").is_none());
        assert!(attr_value(&attrs, "tool.name").is_none());
        assert_eq!(attrs.len(), 1, "only openinference.span.kind expected");
    }
}
