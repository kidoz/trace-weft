//! End-to-end tests for the SDK flow: span builders -> global recorder,
//! including error capture, replay mocks, HITL breakpoints, and trajectory
//! assertions.
//!
//! The SDK recorder is a process-wide singleton, so all tests share one
//! `MemoryStore` and use unique span names to stay isolated from each other.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use trace_weft::eval::{MemoryStore, TraceTrajectory};
use trace_weft::{
    CostEstimate, HitlResponse, ReplayConfig, SpanRecord, SpanStatus, TokenUsage,
    TraceWeftSpanKind, agent, build_agent, build_llm_call, build_tool, init_replay, llm_call,
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
