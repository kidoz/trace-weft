# openrouter-agent

An advanced TraceWeft example: a tool-calling research agent backed by a real
LLM through [OpenRouter](https://openrouter.ai).

Where `basic-agent` shows the minimal recorder setup, this example exercises
the SDK the way a production agent would:

- **`#[agent]` / `#[tool]` macros** — the agent loop and both tools
  (`calculator`, `project_facts`) form an auto-parented span tree with
  argument/return capture.
- **`build_llm_call` + `run_with` around a live HTTP call** — provider, model,
  and input messages are set up front; token usage and credit cost only exist
  on the response, so the closure reports them through the `SpanHandle` and
  they land on the span itself, populating the workbench's Input/Output/Cost
  tiles and token heatmap (the request sets `"usage": {"include": true}` so
  OpenRouter reports cost).
- **`Retry` events** — transient failures (429/5xx, transport errors) back off
  and record a `Retry` event per attempt.
- **`Budget` / `Termination` events** — a token budget is checked before every
  model call; exhausting it (or hitting the step limit) records a
  `Termination` event and aborts the run.
- **`CapturePolicy::FullContentLocalOnly`** — full prompts and tool IO are
  visible in the local workbench; switch to `RedactedPreview` when traces may
  leave your machine.

## Run

The API key is read from the environment — never hardcode or commit it:

```bash
export OPENROUTER_API_KEY=sk-or-...   # https://openrouter.ai/keys
cargo run -p openrouter-agent
# or with your own question:
cargo run -p openrouter-agent -- "How does TraceWeft store traces? Also, what is 17 * 42?"
```

The default model is `anthropic/claude-haiku-4.5`; override it with
`OPENROUTER_MODEL` (pick a model that supports tool calling):

```bash
OPENROUTER_MODEL=openai/gpt-4o-mini cargo run -p openrouter-agent
```

## Inspect the trace

Traces land in `./.trace-weft/`. Start the local API server and the web UI:

```bash
trace-weft dev
npm --prefix apps/web run dev   # then open http://localhost:5173
```

You should see one `research_agent` span with a `chat_completion` LLM-call
span per step (each carrying token usage and cost in the inspector), tool
spans for `calculator` / `project_facts`, and `budget_check` events between
steps.
