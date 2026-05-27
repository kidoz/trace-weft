use trace_weft::{CapturePolicy, LocalConfig, agent, init_local};
use trace_weft_mcp::{mcp_client, mcp_server};

// Simulated Remote MCP Server
async fn simulated_mcp_server(method: &str, traceparent: String) -> anyhow::Result<String> {
    // Reconstruct the context from the incoming network request
    mcp_server(method, Some(&traceparent))
        .server_name("weather-mcp-server")
        .run(|| async move {
            tokio::time::sleep(std::time::Duration::from_millis(30)).await;
            Ok::<String, anyhow::Error>(format!("Simulated response for {}", method))
        })
        .await
}

// Local Agent making an MCP call
#[agent]
async fn run_agent(query: String) -> anyhow::Result<String> {
    // We want to call an external tool. We use the mcp_client builder.
    let mut client_call = mcp_client("weather-mcp-server", "get_weather")
        .client_name("local-agent")
        .request_id("req-123");

    // The client generates a traceparent to pass in headers/metadata
    let traceparent = client_call.inject_traceparent();

    // Execute the client call which wraps the actual network request
    let result = client_call
        .run(|| async move {
            // Send the traceparent over the network...
            // We simulate the network call by calling the simulated server directly
            simulated_mcp_server("get_weather", traceparent).await
        })
        .await?;

    Ok(format!("Agent got: {} for query: {}", result, query))
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

    let result = run_agent("What is the weather in SF?".into()).await?;
    println!("Final Result: {}", result);

    Ok(())
}
