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
    TraceWeftSpanKind, build_agent, build_llm_call, build_tool, init_replay, resolve_approval,
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
