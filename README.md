# TraceWeft

[![Language](https://img.shields.io/badge/language-Rust-blue.svg)](https://www.rust-lang.org/)
[![GitHub License](https://img.shields.io/github/license/kidoz/trace-weft)](https://github.com/kidoz/trace-weft/blob/main/LICENSE)

> TraceWeft is an open-source Rust-first observability and debugging toolkit for LLM agents. It captures model calls, tool calls, memory operations, retrievals, state transitions, checkpoints, handoffs, and errors as structured traces, then lets developers inspect, replay, diff, and export them through OpenTelemetry-compatible pipelines.
>
> TraceWeft is local-first by default: run a Rust agent, open the local debugger, and inspect the full execution without sending prompts or tool outputs to a SaaS service.

## Quickstart

Add `trace-weft` to your Cargo project:

```bash
cargo add trace-weft
```

Install the CLI tool:
```bash
cargo install --path crates/trace-weft-cli
```

## Example Agent

Instrument your Rust agent using procedural macros and manual builders:

```rust
use trace_weft::{agent, init_local, LocalConfig, CapturePolicy};

#[agent]
async fn run_agent(input: String) -> anyhow::Result<String> {
    // LLM operations, tool calls, etc.
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
    
    // Initialize the local background recorder
    init_local(config).await?;

    let result = run_agent("hello world".into()).await?;
    println!("Result: {}", result);

    Ok(())
}
```

## Local Dev Workflow

Once your application produces traces into `.trace-weft/traces.sqlite`, you can inspect them visually:

```bash
trace-weft dev
```

This starts the local `axum` API and React web UI. Navigate to `http://localhost:3000` to view the Trace List, Span Tree, Waterfall, and Replay/Diff UI.

## Crate Layout

- `crates/trace-weft` - main user-facing SDK facade
- `crates/trace-weft-core` - IDs, schemas, span/event types, redaction traits
- `crates/trace-weft-macros` - proc macros: `#[agent]`, `#[tool]`, `#[llm_call]`
- `crates/trace-weft-otel` - OpenTelemetry export/import bridge
- `crates/trace-weft-openinference` - OpenInference compatibility mapping
- `crates/trace-weft-recorder` - local JSONL/SQLite/blob recorder
- `crates/trace-weft-ingest` - OTLP HTTP/gRPC ingestion primitives
- `crates/trace-weft-server` - axum API, query layer, live streaming
- `crates/trace-weft-cli` - CLI: dev, import, export, replay
- `apps/web` - React / TypeScript / Vite UI
