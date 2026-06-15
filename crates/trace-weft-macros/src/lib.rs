use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::{ItemFn, ReturnType, Type, parse_macro_input};

/// Whether the function's declared return type is a `Result<_, _>` (by the last
/// path segment, so `Result`, `std::result::Result`, and `anyhow::Result` all
/// match). Used to decide whether the recorded span can fail.
fn returns_result(sig: &syn::Signature) -> bool {
    let ReturnType::Type(_, ty) = &sig.output else {
        return false;
    };
    matches!(&**ty, Type::Path(tp)
        if tp.path.segments.last().is_some_and(|seg| seg.ident == "Result"))
}

/// Shared expansion for the instrumentation attributes. `kind` is the
/// `TraceWeftSpanKind` variant ident to stamp on the recorded span.
fn expand(kind: TokenStream2, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ItemFn);
    let name = &input.sig.ident;
    let block = &input.block;
    let sig = &input.sig;
    let vis = &input.vis;

    // A `Result`-returning body sets Error status on `Err`; everything else
    // always completes Ok. Only the `Result` arm touches `result` by reference,
    // so a non-`Result` body never gains a spurious `Debug`/`Display` bound.
    let status_update = if returns_result(sig) {
        quote! {
            match &result {
                Ok(_) => { _span.status = trace_weft::SpanStatus::Ok; }
                Err(__e) => {
                    _span.status = trace_weft::SpanStatus::Error;
                    _span.error_type = Some(format!("{:?}", __e));
                    _span.error_message_redacted = Some(format!("{}", __e));
                }
            }
        }
    } else {
        quote! { _span.status = trace_weft::SpanStatus::Ok; }
    };

    let expanded = quote! {
        #vis #sig {
            let mut _span = trace_weft::SpanRecord {
                trace_id: trace_weft::TraceId(trace_weft::uuid::Uuid::now_v7()),
                span_id: trace_weft::SpanId(trace_weft::uuid::Uuid::now_v7()),
                parent_span_id: None,
                run_id: trace_weft::RunId(trace_weft::uuid::Uuid::now_v7()),
                session_id: None,
                user_id_hash: None,
                span_kind: trace_weft::TraceWeftSpanKind::#kind,
                name: stringify!(#name).to_string(),
                start_time: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as u64,
                end_time: None,
                status: trace_weft::SpanStatus::InProgress,
                status_message: None,
                error_type: None,
                error_message_redacted: None,
                attributes: std::collections::HashMap::new(),
                otel_attributes: std::collections::HashMap::new(),
                openinference_attributes: std::collections::HashMap::new(),
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
                redaction_policy: trace_weft::CapturePolicy::MetadataOnly,
                schema_version: "1.0".to_string(),
            };

            if let Some(__parent) = trace_weft::current_span_context() {
                _span.trace_id = __parent.trace_id;
                _span.run_id = __parent.run_id;
                _span.parent_span_id = Some(__parent.span_id);
            }

            let __ctx = trace_weft::SpanContext {
                trace_id: _span.trace_id,
                run_id: _span.run_id,
                span_id: _span.span_id,
            };
            let result = trace_weft::scope_current(__ctx, async move { #block }).await;

            _span.end_time = Some(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as u64);
            _span.latency_ms = Some(_span.end_time.unwrap() - _span.start_time);
            #status_update
            trace_weft::record_span(_span).await;

            result
        }
    };

    TokenStream::from(expanded)
}

#[proc_macro_attribute]
pub fn agent(_attr: TokenStream, item: TokenStream) -> TokenStream {
    expand(quote!(Agent), item)
}

#[proc_macro_attribute]
pub fn tool(_attr: TokenStream, item: TokenStream) -> TokenStream {
    expand(quote!(Tool), item)
}

#[proc_macro_attribute]
pub fn llm_call(_attr: TokenStream, item: TokenStream) -> TokenStream {
    expand(quote!(LlmCall), item)
}
