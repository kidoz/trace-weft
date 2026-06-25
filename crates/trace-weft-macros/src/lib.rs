use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::{FnArg, Ident, ImplItemFn, ItemFn, Pat, ReturnType, Signature, Type};

/// Whether the function's declared return type is a `Result<_, _>` (by the last
/// path segment, so `Result`, `std::result::Result`, and `anyhow::Result` all
/// match). Used to decide whether the recorded span can fail.
fn returns_result(sig: &Signature) -> bool {
    let ReturnType::Type(_, ty) = &sig.output else {
        return false;
    };
    matches!(&**ty, Type::Path(tp)
        if tp.path.segments.last().is_some_and(|seg| seg.ident == "Result"))
}

/// Strip `#[trace(..)]` attributes off the arguments (so the regenerated
/// signature stays valid) and return the idents of arguments to capture: every
/// by-name typed argument not marked `#[trace(skip)]`. The receiver (`self`)
/// and non-ident patterns are never captured.
fn capture_args(sig: &mut Signature) -> Vec<Ident> {
    let mut captured = Vec::new();
    for arg in sig.inputs.iter_mut() {
        let FnArg::Typed(pat_type) = arg else {
            continue;
        };
        let skip = pat_type.attrs.iter().any(|a| a.path().is_ident("trace"));
        pat_type.attrs.retain(|a| !a.path().is_ident("trace"));
        if skip {
            continue;
        }
        if let Pat::Ident(pat_ident) = &*pat_type.pat {
            captured.push(pat_ident.ident.clone());
        }
    }
    captured
}

/// Shared expansion for the instrumentation attributes. `kind` is the
/// `TraceWeftSpanKind` variant ident to stamp on the recorded span.
///
/// Accepts both free functions and `impl`/trait-impl methods (including
/// `&self` receivers). Trait *definitions* carry no body, so the target is the
/// concrete `impl`. The function must be `async`.
fn expand(kind: TokenStream2, item: TokenStream) -> TokenStream {
    let item2 = TokenStream2::from(item);

    // A normal method parses as an `ItemFn`; the `ImplItemFn` fallback covers
    // `default fn` and other impl-only shapes. We keep `attrs` and any
    // `defaultness` so doc comments, stacked attributes, and `default` survive.
    let parsed = syn::parse2::<ItemFn>(item2.clone())
        .map(|f| (f.attrs, f.vis, quote!(), f.sig, *f.block))
        .or_else(|_| {
            syn::parse2::<ImplItemFn>(item2).map(|m| {
                let defaultness = m.defaultness;
                (m.attrs, m.vis, quote!(#defaultness), m.sig, m.block)
            })
        });
    let (attrs, vis, defaultness, mut sig, block) = match parsed {
        Ok(parts) => parts,
        Err(err) => return err.to_compile_error().into(),
    };
    let captured = capture_args(&mut sig);
    let returns_result = returns_result(&sig);
    let name = &sig.ident;

    // Serialize captured args into `input_ref` before the body moves them.
    // Guarded by `capture_enabled()` so a `MetadataOnly` process pays nothing
    // beyond the (compile-time) `Serialize` bound on captured args.
    let input_capture = if captured.is_empty() {
        quote!()
    } else {
        let inserts = captured.iter().map(|id| {
            let key = id.to_string();
            quote! {
                __input.insert(
                    #key.to_string(),
                    trace_weft::serde_json::to_value(&#id)
                        .unwrap_or(trace_weft::serde_json::Value::Null),
                );
            }
        });
        quote! {
            if trace_weft::capture_enabled() {
                let mut __input = trace_weft::serde_json::Map::new();
                #(#inserts)*
                _span.input_ref = trace_weft::capture_json(
                    "application/json",
                    trace_weft::serde_json::Value::Object(__input),
                ).await;
            }
        }
    };

    // Serialize the successful output into `output_ref`.
    let output_capture = if returns_result {
        quote! {
            if trace_weft::capture_enabled() {
                if let Ok(__ok) = &result {
                    _span.output_ref = trace_weft::capture_json(
                        "application/json",
                        trace_weft::serde_json::to_value(__ok)
                            .unwrap_or(trace_weft::serde_json::Value::Null),
                    ).await;
                }
            }
        }
    } else {
        quote! {
            if trace_weft::capture_enabled() {
                _span.output_ref = trace_weft::capture_json(
                    "application/json",
                    trace_weft::serde_json::to_value(&result)
                        .unwrap_or(trace_weft::serde_json::Value::Null),
                ).await;
            }
        }
    };

    // A `Result`-returning body sets Error status on `Err`; everything else
    // always completes Ok. Only the `Result` arm touches `result` by reference,
    // so a non-`Result` body never gains a spurious `Debug`/`Display` bound.
    let status_update = if returns_result {
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
        #(#attrs)*
        #vis #defaultness #sig {
            let mut _span = trace_weft::SpanRecord {
                trace_id: trace_weft::TraceId(trace_weft::uuid::Uuid::now_v7()),
                span_id: trace_weft::SpanId(trace_weft::uuid::Uuid::now_v7()),
                parent_span_id: None,
                run_id: trace_weft::RunId(trace_weft::uuid::Uuid::now_v7()),
                session_id: None,
                user_id_hash: None,
                project_id: None,
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
                redaction_policy: trace_weft::capture_policy(),
                schema_version: "1.0".to_string(),
            };

            if let Some(__parent) = trace_weft::current_span_context() {
                _span.trace_id = __parent.trace_id;
                _span.run_id = __parent.run_id;
                _span.parent_span_id = Some(__parent.span_id);
            }

            #input_capture

            let __ctx = trace_weft::SpanContext {
                trace_id: _span.trace_id,
                run_id: _span.run_id,
                span_id: _span.span_id,
            };
            let result = trace_weft::scope_current(__ctx, async move #block).await;

            _span.end_time = Some(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as u64);
            _span.latency_ms = Some(_span.end_time.unwrap() - _span.start_time);
            #status_update
            #output_capture
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
