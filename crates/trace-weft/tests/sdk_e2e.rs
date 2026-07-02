//! End-to-end tests for the SDK flow: span builders -> global recorder,
//! including error capture, replay mocks, HITL breakpoints, and trajectory
//! assertions.
//!
//! The SDK recorder is a process-wide singleton, so all tests share one
//! `MemoryStore` and use unique span names to stay isolated from each other.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use serde::Serialize;
use trace_weft::capture::MemoryBlobStore;
use trace_weft::eval::{MemoryStore, TraceTrajectory};
use trace_weft::{
    BlobStore, CaptureConfig, CapturePolicy, CostEstimate, EventKind, EventRecord, HitlResponse,
    RedactionStatus, ReplayConfig, SpanRecord, SpanStatus, TokenUsage, TraceWeftSpanKind, agent,
    build_agent, build_llm_call, build_tool, event, init_capture, init_replay, llm_call,
    resolve_approval, tool,
};

fn store() -> &'static MemoryStore {
    static STORE: OnceLock<MemoryStore> = OnceLock::new();
    STORE.get_or_init(|| {
        let store = MemoryStore::new();
        trace_weft::init_custom(std::sync::Arc::new(store.clone()))
            .expect("recorder initialized once for the test process");
        store
    })
}

fn recorded_spans_named(name: &str) -> Vec<SpanRecord> {
    store()
        .spans
        .lock()
        .unwrap()
        .iter()
        .filter(|s| s.name == name)
        .cloned()
        .collect()
}

fn recorded_events_named(name: &str) -> Vec<EventRecord> {
    store()
        .events
        .lock()
        .unwrap()
        .iter()
        .filter(|e| e.name == name)
        .cloned()
        .collect()
}

fn capture_blobs() -> &'static MemoryBlobStore {
    static BLOBS: OnceLock<MemoryBlobStore> = OnceLock::new();
    BLOBS.get_or_init(|| {
        let blobs = MemoryBlobStore::new();
        init_capture(CaptureConfig {
            policy: CapturePolicy::RedactedPreview,
            blobs: std::sync::Arc::new(blobs.clone()),
            redactor: std::sync::Arc::new(trace_weft::redactor::RegexRedactor::default()),
            storage_backend: "memory".to_string(),
        })
        .expect("capture initialized once for the test process");
        blobs
    })
}

#[tokio::test]
async fn llm_call_builder_records_success_span() {
    store();
    let result = build_llm_call("e2e_llm_ok")
        .provider("openai")
        .model("gpt-4.1")
        .prompt_version("v7")
        .run(|| async { Ok::<i32, String>(42) })
        .await;

    assert_eq!(result, Ok(42));

    let spans = recorded_spans_named("e2e_llm_ok");
    assert_eq!(spans.len(), 1);
    let span = &spans[0];
    assert_eq!(span.span_kind, TraceWeftSpanKind::LlmCall);
    assert_eq!(span.status, SpanStatus::Ok);
    assert_eq!(span.model_provider.as_deref(), Some("openai"));
    assert_eq!(span.model_name.as_deref(), Some("gpt-4.1"));
    assert_eq!(span.prompt_version.as_deref(), Some("v7"));
    assert!(span.end_time.is_some());
    assert!(span.latency_ms.is_some());
}

#[tokio::test]
async fn tool_builder_records_error_span() {
    store();
    let result = build_tool("e2e_tool_err")
        .tool_name("kb_search")
        .run(|| async { Err::<i32, String>("boom".to_string()) })
        .await;

    assert_eq!(result, Err("boom".to_string()));

    let spans = recorded_spans_named("e2e_tool_err");
    assert_eq!(spans.len(), 1);
    let span = &spans[0];
    assert_eq!(span.span_kind, TraceWeftSpanKind::Tool);
    assert_eq!(span.status, SpanStatus::Error);
    assert_eq!(span.error_message_redacted.as_deref(), Some("boom"));
    assert!(span.error_type.is_some());
}

#[tokio::test]
async fn builder_redacts_error_messages_without_content_capture() {
    store();
    let result = build_tool("e2e_tool_secret_err")
        .tool_name("dangerous_tool")
        .run(|| async {
            Err::<(), String>("failed with api_key = tw_abcdefghijklmnopqrstuvwxyz".to_string())
        })
        .await;

    assert!(result.is_err());

    let spans = recorded_spans_named("e2e_tool_secret_err");
    assert_eq!(spans.len(), 1);
    let span = &spans[0];
    let message = span
        .error_message_redacted
        .as_deref()
        .expect("redacted error message recorded");
    assert_eq!(message, "failed with [REDACTED]");
    assert!(!message.contains("tw_abcdefghijklmnopqrstuvwxyz"));
    assert_ne!(
        span.error_type.as_deref(),
        Some("failed with api_key = tw_abcdefghijklmnopqrstuvwxyz")
    );
}

#[tokio::test]
async fn builder_setters_populate_rich_span_fields() {
    store();
    let usage = TokenUsage {
        input: 100,
        output: 40,
        reasoning: Some(10),
        breakdown: Default::default(),
    };
    let cost = CostEstimate {
        currency: "USD".into(),
        amount: 0.05,
    };

    build_llm_call("e2e_rich_fields")
        .provider("anthropic")
        .model("claude-fable-5")
        .token_usage(usage.clone())
        .cost(cost.clone())
        .cache_hit(true)
        .retrieval("sha256:query", vec![])
        .attribute("region", serde_json::json!("us-east-1"))
        .run(|| async { Ok::<i32, String>(1) })
        .await
        .unwrap();

    let spans = recorded_spans_named("e2e_rich_fields");
    assert_eq!(spans.len(), 1);
    let span = &spans[0];
    assert_eq!(span.token_usage.as_ref(), Some(&usage));
    assert_eq!(span.cost_estimate.as_ref(), Some(&cost));
    assert_eq!(span.cache_hit, Some(true));
    assert_eq!(span.retrieval_query_hash.as_deref(), Some("sha256:query"));
    assert_eq!(
        span.attributes.get("region"),
        Some(&serde_json::json!("us-east-1"))
    );
}

#[tokio::test]
async fn builder_captures_labeled_refs_under_policy() {
    store();
    let blobs = capture_blobs();

    build_tool("e2e_builder_capture")
        .input_ref("query", &serde_json::json!({"email": "dev@example.com"}))
        .output_ref("result", &serde_json::json!({"phone": "+1 (415) 555-2671"}))
        .run(|| async { Ok::<_, String>("done".to_string()) })
        .await
        .unwrap();

    let spans = recorded_spans_named("e2e_builder_capture");
    assert_eq!(spans.len(), 1);
    let span = &spans[0];
    assert_eq!(span.redaction_policy, CapturePolicy::RedactedPreview);

    let input = span.input_ref.as_ref().expect("input captured");
    let input_preview = input.preview_text_redacted.as_ref().unwrap();
    assert!(input_preview.contains("query"));
    assert!(input_preview.contains("[REDACTED]"));
    assert!(!input_preview.contains("dev@example.com"));
    assert_eq!(input.redaction_status, RedactionStatus::Redacted);

    let output = span.output_ref.as_ref().expect("output captured");
    let output_preview = output.preview_text_redacted.as_ref().unwrap();
    assert!(output_preview.contains("result"));
    assert!(output_preview.contains("[REDACTED]"));
    assert!(!output_preview.contains("415"));
    assert_eq!(output.redaction_status, RedactionStatus::Redacted);

    let input_blob = blobs
        .get_blob(&input.hash)
        .await
        .unwrap()
        .expect("input blob persisted");
    let input_blob = String::from_utf8(input_blob).unwrap();
    assert!(input_blob.contains("[REDACTED]"));
    assert!(!input_blob.contains("dev@example.com"));
}

#[agent]
async fn macro_agent_fn() -> Result<u8, String> {
    Ok(1)
}

#[tool]
async fn macro_tool_fn() -> Result<u8, String> {
    Ok(2)
}

#[llm_call]
async fn macro_llm_fn() -> Result<u8, String> {
    Ok(3)
}

#[tokio::test]
async fn events_auto_link_to_ambient_span() {
    store();
    build_agent("e2e_event_root")
        .run(|| async {
            event(EventKind::Budget, "e2e_budget_check")
                .attribute("tokens_remaining", serde_json::json!(1500))
                .record()
                .await;
            event(EventKind::Retry, "e2e_retry").record().await;
            Ok::<(), String>(())
        })
        .await
        .unwrap();

    let root = &recorded_spans_named("e2e_event_root")[0];

    let budget = recorded_events_named("e2e_budget_check");
    assert_eq!(budget.len(), 1);
    assert_eq!(budget[0].event_kind, EventKind::Budget);
    assert_eq!(budget[0].parent_span_id, Some(root.span_id));
    assert_eq!(budget[0].trace_id, root.trace_id);
    assert_eq!(budget[0].run_id, root.run_id);
    assert_eq!(
        budget[0].attributes.get("tokens_remaining"),
        Some(&serde_json::json!(1500))
    );

    let retry = recorded_events_named("e2e_retry");
    assert_eq!(retry.len(), 1);
    assert_eq!(retry[0].parent_span_id, Some(root.span_id));
    // Events carry a monotonic ordering hint.
    assert!(retry[0].seq > budget[0].seq);
}

#[tokio::test]
async fn run_with_enriches_span_from_inside_closure() {
    store();
    let value = build_llm_call("e2e_run_with_enrich")
        .provider("openrouter")
        .run_with(|span| async move {
            // Simulates usage/cost that only exist on the provider response.
            span.token_usage(TokenUsage {
                input: 1008,
                output: 148,
                reasoning: None,
                breakdown: Default::default(),
            });
            span.cost(CostEstimate {
                currency: "USD".into(),
                amount: 0.001748,
            });
            span.cache_hit(false);
            span.attribute("finish_reason", serde_json::json!("stop"));
            Ok::<i32, String>(7)
        })
        .await;
    assert_eq!(value, Ok(7));

    let spans = recorded_spans_named("e2e_run_with_enrich");
    assert_eq!(spans.len(), 1);
    let span = &spans[0];
    assert_eq!(span.status, SpanStatus::Ok);
    let usage = span.token_usage.as_ref().expect("usage set via handle");
    assert_eq!(usage.input, 1008);
    assert_eq!(usage.output, 148);
    let cost = span.cost_estimate.as_ref().expect("cost set via handle");
    assert!((cost.amount - 0.001748).abs() < f64::EPSILON);
    assert_eq!(span.cache_hit, Some(false));
    assert_eq!(
        span.attributes.get("finish_reason"),
        Some(&serde_json::json!("stop"))
    );
}

#[tokio::test]
async fn run_with_applies_enrichment_on_error_and_overrides_setters() {
    store();
    let result = build_llm_call("e2e_run_with_err")
        .token_usage(TokenUsage {
            input: 1,
            output: 1,
            reasoning: None,
            breakdown: Default::default(),
        })
        .run_with(|span| async move {
            // Partial usage reported before the call failed must survive, and
            // must win over the builder's pre-set value.
            span.token_usage(TokenUsage {
                input: 500,
                output: 0,
                reasoning: None,
                breakdown: Default::default(),
            });
            Err::<(), String>("upstream timeout".to_string())
        })
        .await;
    assert!(result.is_err());

    let spans = recorded_spans_named("e2e_run_with_err");
    assert_eq!(spans.len(), 1);
    let span = &spans[0];
    assert_eq!(span.status, SpanStatus::Error);
    let usage = span.token_usage.as_ref().expect("usage kept on error");
    assert_eq!(usage.input, 500);
}

#[tokio::test]
async fn run_infallible_records_ok_span_for_non_result_closure() {
    store();
    let value = build_agent("e2e_infallible")
        .run_infallible(|| async { 42u8 })
        .await;
    assert_eq!(value, 42);

    let spans = recorded_spans_named("e2e_infallible");
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0].status, SpanStatus::Ok);
    assert!(spans[0].latency_ms.is_some());
}

#[tokio::test]
async fn event_without_ambient_context_is_unparented() {
    store();
    event(EventKind::Log, "e2e_orphan_event").record().await;

    let events = recorded_events_named("e2e_orphan_event");
    assert_eq!(events.len(), 1);
    assert!(events[0].parent_span_id.is_none());
}

#[tokio::test]
async fn builder_children_auto_link_to_ambient_parent() {
    store();
    build_agent("e2e_ambient_root")
        .run(|| async {
            build_tool("e2e_ambient_child")
                .run(|| async { Ok::<(), String>(()) })
                .await
        })
        .await
        .unwrap();

    let root = &recorded_spans_named("e2e_ambient_root")[0];
    let child = &recorded_spans_named("e2e_ambient_child")[0];

    assert_eq!(child.parent_span_id, Some(root.span_id));
    assert_eq!(child.trace_id, root.trace_id);
    assert_eq!(child.run_id, root.run_id);
    assert!(
        root.parent_span_id.is_none(),
        "root span must stay parentless"
    );
}

#[tokio::test]
async fn with_parent_overrides_ambient_context() {
    store();
    let explicit = build_agent("e2e_explicit_parent");
    let trace_id = explicit.span.trace_id;
    let run_id = explicit.span.run_id;
    let parent_id = explicit.span.span_id;

    build_agent("e2e_ambient_wrapper")
        .run(|| async {
            build_tool("e2e_explicit_child")
                .with_parent(trace_id, run_id, parent_id)
                .run(|| async { Ok::<(), String>(()) })
                .await
        })
        .await
        .unwrap();

    let child = &recorded_spans_named("e2e_explicit_child")[0];
    assert_eq!(child.parent_span_id, Some(parent_id));
    assert_eq!(child.trace_id, trace_id);
    assert_eq!(child.run_id, run_id);
}

#[agent]
async fn macro_outer_fn() -> Result<(), String> {
    macro_inner_fn().await
}

#[tool]
async fn macro_inner_fn() -> Result<(), String> {
    Ok(())
}

#[tokio::test]
async fn macros_auto_link_to_ambient_parent() {
    store();
    macro_outer_fn().await.unwrap();

    let outer = &recorded_spans_named("macro_outer_fn")[0];
    let inner = &recorded_spans_named("macro_inner_fn")[0];

    assert_eq!(inner.parent_span_id, Some(outer.span_id));
    assert_eq!(inner.trace_id, outer.trace_id);
    assert_eq!(inner.run_id, outer.run_id);
}

#[tool]
async fn macro_failing_fn() -> Result<u8, String> {
    Err("kaboom".to_string())
}

struct MacroCalc {
    base: u8,
}

impl MacroCalc {
    /// Doc comments and other attributes on the method must survive expansion.
    #[tool]
    async fn add(&self, x: u8) -> Result<u8, String> {
        Ok(self.base + x)
    }
}

trait MacroGreeter {
    async fn greet(&self) -> Result<String, String>;
}

impl MacroGreeter for MacroCalc {
    #[llm_call]
    async fn greet(&self) -> Result<String, String> {
        Ok(format!("base={}", self.base))
    }
}

#[tokio::test]
async fn macros_instrument_impl_methods() {
    store();
    let calc = MacroCalc { base: 10 };

    assert_eq!(calc.add(5).await, Ok(15));
    assert_eq!(calc.greet().await, Ok("base=10".to_string()));

    let add_spans = recorded_spans_named("add");
    assert_eq!(add_spans.len(), 1);
    assert_eq!(add_spans[0].span_kind, TraceWeftSpanKind::Tool);

    let greet_spans = recorded_spans_named("greet");
    assert_eq!(greet_spans.len(), 1);
    assert_eq!(greet_spans[0].span_kind, TraceWeftSpanKind::LlmCall);
}

#[tokio::test]
async fn macro_records_error_status_on_err() {
    store();
    let result = macro_failing_fn().await;
    assert_eq!(result, Err("kaboom".to_string()));

    let spans = recorded_spans_named("macro_failing_fn");
    assert_eq!(spans.len(), 1);
    let span = &spans[0];
    assert_eq!(span.status, SpanStatus::Error);
    assert_eq!(span.error_message_redacted.as_deref(), Some("kaboom"));
    assert!(span.error_type.is_some());
}

#[tool]
async fn macro_error_with_secret_fn() -> Result<(), String> {
    Err("failed with Bearer abc.DEF-123~xyz".to_string())
}

#[tokio::test]
async fn macros_redact_error_messages_without_content_capture() {
    store();
    let result = macro_error_with_secret_fn().await;
    assert!(result.is_err());

    let spans = recorded_spans_named("macro_error_with_secret_fn");
    assert_eq!(spans.len(), 1);
    let span = &spans[0];
    let message = span
        .error_message_redacted
        .as_deref()
        .expect("redacted error message recorded");
    assert_eq!(message, "failed with [REDACTED]");
    assert!(!message.contains("abc.DEF"));
    let error_type = span.error_type.as_deref().expect("error type recorded");
    assert!(!error_type.contains("abc.DEF"));
}

#[derive(Serialize)]
struct CapturePayload {
    msg: String,
}

#[tool]
async fn macro_capturing_fn(
    payload: CapturePayload,
    #[trace(skip)] _secret: String,
) -> Result<String, String> {
    Ok(format!("ok:{}", payload.msg))
}

#[tokio::test]
async fn macros_capture_inputs_and_outputs_under_policy() {
    store();
    let blobs = capture_blobs();

    let out = macro_capturing_fn(
        CapturePayload {
            msg: "contact a@b.com".to_string(),
        },
        "topsecret".to_string(),
    )
    .await;
    assert_eq!(out, Ok("ok:contact a@b.com".to_string()));

    let spans = recorded_spans_named("macro_capturing_fn");
    assert_eq!(spans.len(), 1);
    let span = &spans[0];
    assert_eq!(span.redaction_policy, CapturePolicy::RedactedPreview);

    let input = span.input_ref.as_ref().expect("input captured");
    let preview = input.preview_text_redacted.as_ref().unwrap();
    assert!(preview.contains("payload"), "captured arg key present");
    assert!(preview.contains("[REDACTED]"), "email redacted: {preview}");
    assert!(!preview.contains("a@b.com"));
    assert!(
        !preview.contains("topsecret") && !preview.contains("_secret"),
        "#[trace(skip)] arg must be excluded: {preview}"
    );
    assert_eq!(input.redaction_status, RedactionStatus::Redacted);

    assert!(span.output_ref.is_some(), "output captured");
    assert!(blobs.len() >= 2, "input and output blobs persisted");
}

#[tokio::test]
async fn macros_record_their_own_span_kind() {
    store();
    macro_agent_fn().await.unwrap();
    macro_tool_fn().await.unwrap();
    macro_llm_fn().await.unwrap();

    let agent_spans = recorded_spans_named("macro_agent_fn");
    assert_eq!(agent_spans.len(), 1);
    assert_eq!(agent_spans[0].span_kind, TraceWeftSpanKind::Agent);

    let tool_spans = recorded_spans_named("macro_tool_fn");
    assert_eq!(tool_spans.len(), 1);
    assert_eq!(tool_spans[0].span_kind, TraceWeftSpanKind::Tool);

    let llm_spans = recorded_spans_named("macro_llm_fn");
    assert_eq!(llm_spans.len(), 1);
    assert_eq!(llm_spans[0].span_kind, TraceWeftSpanKind::LlmCall);
}

#[tokio::test]
async fn replay_mock_short_circuits_execution() {
    store();
    let mut config = ReplayConfig::default();
    config
        .mocked_spans
        .insert("e2e_mocked".to_string(), serde_json::json!(99));
    init_replay(config);

    static EXECUTED: AtomicBool = AtomicBool::new(false);
    let result = build_llm_call("e2e_mocked")
        .run(|| async {
            EXECUTED.store(true, Ordering::SeqCst);
            Ok::<i32, String>(1)
        })
        .await;

    assert_eq!(result, Ok(99), "mocked output should replace the real call");
    assert!(
        !EXECUTED.load(Ordering::SeqCst),
        "mocked span must not execute the real closure"
    );

    let spans = recorded_spans_named("e2e_mocked");
    assert_eq!(spans.len(), 1);
    let span = &spans[0];
    assert_eq!(span.status, SpanStatus::Ok);
    assert_eq!(span.latency_ms, Some(0));
    assert_eq!(
        span.attributes.get("replayed"),
        Some(&serde_json::json!(true))
    );
}

async fn wait_for_pending(span_id: &str) {
    for _ in 0..100 {
        if trace_weft::get_pending_approvals().contains(&span_id.to_string()) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("span {span_id} never appeared in pending approvals");
}

#[tokio::test]
async fn hitl_approval_resumes_execution() {
    store();
    let builder = build_agent("e2e_hitl_approve");
    let span_id = builder.span.span_id.0.to_string();

    let handle = tokio::spawn(builder.wait_for_approval());

    wait_for_pending(&span_id).await;
    resolve_approval(
        &span_id,
        HitlResponse::Approved(serde_json::json!({"x": 1})),
    )
    .unwrap();

    let response = handle.await.unwrap().unwrap();
    match response {
        HitlResponse::Approved(value) => assert_eq!(value, serde_json::json!({"x": 1})),
        HitlResponse::Rejected(reason) => panic!("expected approval, got rejection: {reason}"),
    }

    // The span is recorded twice: once pending, once completed.
    let spans = recorded_spans_named("e2e_hitl_approve");
    assert_eq!(spans.len(), 2);
    assert_eq!(spans[0].status, SpanStatus::PendingApproval);
    assert_eq!(spans[1].status, SpanStatus::Ok);
    assert!(spans[1].end_time.is_some());
}

#[tokio::test]
async fn hitl_rejection_is_delivered() {
    let builder = build_agent("e2e_hitl_reject");
    let span_id = builder.span.span_id.0.to_string();

    let handle = tokio::spawn(builder.wait_for_approval());

    wait_for_pending(&span_id).await;
    resolve_approval(&span_id, HitlResponse::Rejected("too risky".to_string())).unwrap();

    let response = handle.await.unwrap().unwrap();
    match response {
        HitlResponse::Rejected(reason) => assert_eq!(reason, "too risky"),
        HitlResponse::Approved(_) => panic!("expected rejection, got approval"),
    }
}

#[tokio::test]
async fn resolving_unknown_approval_fails() {
    let result = resolve_approval(
        "00000000-0000-0000-0000-00000000dead",
        HitlResponse::Approved(serde_json::json!({})),
    );
    assert!(result.is_err());
}

#[tokio::test]
async fn trajectory_assertions_over_recorded_run() {
    store();
    let mut root = build_agent("e2e_traj_root");
    root.span.cost_estimate = Some(CostEstimate {
        currency: "USD".into(),
        amount: 0.01,
    });
    let trace_id = root.span.trace_id;
    let run_id = root.span.run_id;
    let root_span_id = root.span.span_id;

    let mut llm = build_llm_call("e2e_traj_llm").with_parent(trace_id, run_id, root_span_id);
    llm.span.token_usage = Some(TokenUsage {
        input: 120,
        output: 30,
        reasoning: None,
        breakdown: Default::default(),
    });
    llm.span.cost_estimate = Some(CostEstimate {
        currency: "USD".into(),
        amount: 0.02,
    });
    llm.run(|| async { Ok::<(), String>(()) }).await.unwrap();

    build_tool("e2e_traj_tool")
        .with_parent(trace_id, run_id, root_span_id)
        .run(|| async { Ok::<(), String>(()) })
        .await
        .unwrap();

    root.run(|| async { Ok::<(), String>(()) }).await.unwrap();

    // Assemble the trajectory for this trace only; the store is shared across tests.
    let spans: Vec<SpanRecord> = store()
        .spans
        .lock()
        .unwrap()
        .iter()
        .filter(|s| s.trace_id == trace_id)
        .cloned()
        .collect();
    assert_eq!(spans.len(), 3);

    let trajectory = TraceTrajectory { spans };
    assert!(trajectory.contains_tool_call("e2e_traj_tool"));
    assert!(!trajectory.contains_tool_call("drop_table"));
    assert!(!trajectory.has_errors());
    assert_eq!(trajectory.total_input_tokens(), 120);
    assert!((trajectory.total_cost() - 0.03).abs() < f64::EPSILON);
}
