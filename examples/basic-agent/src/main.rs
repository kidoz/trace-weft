use trace_weft::{CapturePolicy, LocalConfig, agent, init_local};

#[agent]
async fn run_agent(input: String) -> anyhow::Result<String> {
    // Add a small delay to simulate work
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    Ok(format!("Agent processed: {}", input))
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = LocalConfig {
        database_path: "./.trace-weft/traces.jsonl".into(),
        sqlite_db_path: "./.trace-weft/traces.sqlite".into(),
        blob_dir: "./.trace-weft/blobs".into(),
        capture_content: CapturePolicy::RedactedPreview,
    };
    init_local(config).await?;

    let args: Vec<String> = std::env::args().collect();
    let input = if args.len() > 1 {
        args[1].clone()
    } else {
        "hello world".into()
    };

    let result = run_agent(input).await?;
    println!("Result: {}", result);

    Ok(())
}
