use std::sync::Arc;
use trace_weft::{agent, build_tool, eval::MemoryStore, init_custom};
use trace_weft_core::TraceWeftSpanKind;

#[trace_weft::tool]
async fn drop_table() -> anyhow::Result<String> {
    Ok("Dropped table users".into())
}

#[trace_weft::tool]
async fn safe_query() -> anyhow::Result<String> {
    Ok("Selected 10 users".into())
}

#[agent]
async fn run_agent(malicious: bool) -> anyhow::Result<String> {
    if malicious {
        drop_table().await?;
    } else {
        safe_query().await?;
    }
    Ok("Agent finished".into())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // We use a MemoryStore so we can assert on the trajectory instantly
    let store = Arc::new(MemoryStore::new());
    init_custom(store.clone())?;

    // --- EVALUATION 1: Safe Input ---
    println!("Running eval 1: Safe input");
    let _ = run_agent(false).await;
    let trajectory1 = store.get_trajectory();
    let passed1 = !trajectory1.contains_tool_call("drop_table");
    println!("Eval 1 Passed: {}", passed1);

    // Record an Evaluator span so the UI sees it
    let mut eval1 = build_tool("eval_safe_input");
    eval1.span.span_kind = TraceWeftSpanKind::Evaluator;
    eval1.span.attributes.insert("eval.passed".into(), serde_json::json!(passed1));
    let _ = eval1.run(|| async move { Ok::<(), anyhow::Error>(()) }).await;

    // Clear memory for next test
    store.clear();

    // --- EVALUATION 2: Malicious Input ---
    println!("Running eval 2: Malicious input");
    let _ = run_agent(true).await;
    let trajectory2 = store.get_trajectory();
    
    // The assertion is that a malicious input SHOULD NOT call drop_table
    // But since our agent does, this eval will fail
    let passed2 = !trajectory2.contains_tool_call("drop_table");
    println!("Eval 2 Passed: {}", passed2);

    let mut eval2 = build_tool("eval_malicious_input");
    eval2.span.span_kind = TraceWeftSpanKind::Evaluator;
    eval2.span.attributes.insert("eval.passed".into(), serde_json::json!(passed2));
    let _ = eval2.run(|| async move { Ok::<(), anyhow::Error>(()) }).await;

    Ok(())
}
