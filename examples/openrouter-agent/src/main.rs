//! An advanced TraceWeft example: a tool-calling research agent backed by
//! OpenRouter (<https://openrouter.ai>).
//!
//! What it demonstrates on top of `basic-agent`:
//! - `#[agent]` / `#[tool]` macros forming a real span tree around a live LLM;
//! - `build_llm_call` + `run_with` wrapping a real HTTP call, with
//!   provider/model/input capture set up front and response-derived token
//!   usage + cost reported through the `SpanHandle` onto the span itself;
//! - `Retry` events around transient HTTP failures (429/5xx) with backoff;
//! - a token budget enforced across the agent loop with `Budget` events and a
//!   `Termination` event when the loop gives up.
//!
//! Run it (the key is read from the environment, never hardcoded):
//!
//! ```bash
//! OPENROUTER_API_KEY=sk-or-... cargo run -p openrouter-agent -- "your question"
//! ```
//!
//! Then inspect the trace with `trace-weft dev` plus the web UI.

use std::time::Duration;

use anyhow::{Context, anyhow, bail};
use serde::{Deserialize, Serialize};
use serde_json::json;
use trace_weft::{
    CapturePolicy, CostEstimate, EventKind, LocalConfig, TokenUsage, agent, build_llm_call, event,
    init_local, tool,
};

const OPENROUTER_URL: &str = "https://openrouter.ai/api/v1/chat/completions";
const DEFAULT_MODEL: &str = "anthropic/claude-haiku-4.5";
const MAX_STEPS: usize = 8;
const MAX_ATTEMPTS: u32 = 3;
const TOKEN_BUDGET: u64 = 24_000;

const SYSTEM_PROMPT: &str = "You are a research assistant instrumented with TraceWeft. \
Use the project_facts tool for any claim about TraceWeft and the calculator tool for any \
arithmetic - never compute numbers yourself. When you have everything you need, reply \
with a short final answer.";

const DEFAULT_QUESTION: &str = "Use project_facts to learn what TraceWeft records, then use \
the calculator to work out how many spans 3 agents produce if each makes 12 LLM calls and \
every call also triggers 2 tool spans. Summarize both findings.";

// --- OpenRouter wire types (OpenAI-compatible chat completions) ---

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ChatMessage {
    role: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    content: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<ToolCall>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
}

impl ChatMessage {
    fn text(role: &str, content: impl Into<String>) -> Self {
        Self {
            role: role.into(),
            content: Some(content.into()),
            tool_calls: None,
            tool_call_id: None,
        }
    }

    fn tool_result(call_id: &str, content: String) -> Self {
        Self {
            role: "tool".into(),
            content: Some(content),
            tool_calls: None,
            tool_call_id: Some(call_id.into()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ToolCall {
    id: String,
    #[serde(rename = "type")]
    kind: String,
    function: FunctionCall,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct FunctionCall {
    name: String,
    /// JSON-encoded arguments, as produced by the model.
    arguments: String,
}

#[derive(Debug, Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>,
    #[serde(default)]
    usage: Option<Usage>,
}

#[derive(Debug, Deserialize)]
struct Choice {
    message: ChatMessage,
}

#[derive(Debug, Deserialize)]
struct Usage {
    prompt_tokens: u64,
    completion_tokens: u64,
    /// Filled in by OpenRouter when the request asks for `usage.include`.
    #[serde(default)]
    cost: Option<f64>,
}

// --- OpenRouter client, instrumented with `build_llm_call` ---

struct OpenRouterClient {
    http: reqwest::Client,
    api_key: String,
    model: String,
}

impl OpenRouterClient {
    fn from_env() -> anyhow::Result<Self> {
        let api_key = std::env::var("OPENROUTER_API_KEY")
            .context("set OPENROUTER_API_KEY (create one at https://openrouter.ai/keys)")?;
        let model = std::env::var("OPENROUTER_MODEL").unwrap_or_else(|_| DEFAULT_MODEL.to_string());
        Ok(Self {
            http: reqwest::Client::new(),
            api_key,
            model,
        })
    }

    /// One chat-completion turn, recorded as an `LlmCall` span. Token usage and
    /// cost only exist on the response, so the closure reports them through the
    /// `SpanHandle` and `run_with` merges them into the span when the call
    /// finishes.
    async fn chat(&self, messages: &[ChatMessage]) -> anyhow::Result<ChatResponse> {
        let body = json!({
            "model": self.model,
            "messages": messages,
            "tools": tool_schemas(),
            // Ask OpenRouter to include credit cost in the usage block.
            "usage": { "include": true },
        });
        build_llm_call("chat_completion")
            .provider("openrouter")
            .model(&self.model)
            .input_ref("messages", &messages)
            .attribute("message_count", json!(messages.len()))
            .run_with(|span| async move {
                let response = self.post_with_retry(&body).await?;
                if let Some(usage) = &response.usage {
                    span.token_usage(TokenUsage {
                        input: usage.prompt_tokens,
                        output: usage.completion_tokens,
                        reasoning: None,
                        breakdown: Default::default(),
                    });
                    if let Some(cost) = usage.cost {
                        span.cost(CostEstimate {
                            currency: "USD".into(),
                            amount: cost,
                        });
                    }
                }
                Ok::<_, anyhow::Error>(response)
            })
            .await
    }

    /// POST with backoff on transient failures, recording a `Retry` event per
    /// attempt that is retried.
    async fn post_with_retry(&self, body: &serde_json::Value) -> anyhow::Result<ChatResponse> {
        for attempt in 1..=MAX_ATTEMPTS {
            let sent = self
                .http
                .post(OPENROUTER_URL)
                .bearer_auth(&self.api_key)
                .header("X-Title", "trace-weft openrouter-agent example")
                .json(body)
                .send()
                .await;
            let reason = match sent {
                Ok(response) if response.status().is_success() => {
                    return Ok(response.json::<ChatResponse>().await?);
                }
                Ok(response) => {
                    let status = response.status();
                    let detail = response.text().await.unwrap_or_default();
                    if status.as_u16() != 429 && !status.is_server_error() {
                        bail!("OpenRouter request failed with {status}: {detail}");
                    }
                    format!("HTTP {status}: {detail}")
                }
                Err(err) => format!("transport error: {err}"),
            };
            if attempt == MAX_ATTEMPTS {
                bail!("OpenRouter request failed after {MAX_ATTEMPTS} attempts: {reason}");
            }
            event(EventKind::Retry, "chat_completion_retry")
                .attribute("attempt", json!(attempt))
                .attribute("reason", json!(reason))
                .record()
                .await;
            tokio::time::sleep(Duration::from_millis(300 * 2u64.pow(attempt))).await;
        }
        unreachable!("the retry loop either returns or bails")
    }
}

// --- Tools the model can call ---

fn tool_schemas() -> serde_json::Value {
    json!([
        {
            "type": "function",
            "function": {
                "name": "calculator",
                "description": "Evaluate an arithmetic expression with +, -, *, / and parentheses.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "expression": {
                            "type": "string",
                            "description": "The expression to evaluate, e.g. (12000 * 0.75) / 3"
                        }
                    },
                    "required": ["expression"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "project_facts",
                "description": "Look up facts about the TraceWeft observability toolkit by topic.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "topic": {
                            "type": "string",
                            "description": "One of: capture, replay, spans, events, storage, overview"
                        }
                    },
                    "required": ["topic"]
                }
            }
        }
    ])
}

#[tool]
async fn calculator(expression: String) -> anyhow::Result<String> {
    let value = eval_expression(&expression)?;
    Ok(value.to_string())
}

#[tool]
async fn project_facts(topic: String) -> anyhow::Result<String> {
    let normalized = topic.to_lowercase();
    let fact = if normalized.contains("capture") {
        "TraceWeft capture policies: MetadataOnly (no content), RedactedPreview (redacted \
         content plus preview), and FullContentLocalOnly / FullContentExportable (full \
         content, redacted preview). Captured content is hashed into a blob store."
    } else if normalized.contains("replay") {
        "TraceWeft replay short-circuits SpanBuilder::run to a mocked output, letting a \
         recorded trajectory be re-executed without live model calls."
    } else if normalized.contains("span") {
        "TraceWeft records model calls, tool calls, memory operations, retrievals, state \
         transitions, checkpoints, handoffs, and errors as structured spans with latency, \
         status, and automatic parent linking."
    } else if normalized.contains("event") {
        "TraceWeft events are point-in-time occurrences inside a span: LlmCall, ToolCall, \
         ReplExec, Rpc, Budget, Guardrail, Retry, Termination, Log, and Custom."
    } else if normalized.contains("stor") {
        "TraceWeft is local-first: a JSONL stream plus a SQLite mirror and a content-addressed \
         blob store; the server also supports Postgres with API-key tenant scoping."
    } else {
        "TraceWeft is an open-source Rust-first observability and debugging toolkit for LLM \
         agents: capture spans and events locally, then inspect, replay, diff, and export \
         them through OpenTelemetry-compatible pipelines."
    };
    Ok(fact.to_string())
}

async fn dispatch_tool_call(call: &ToolCall) -> anyhow::Result<String> {
    let args: serde_json::Value =
        serde_json::from_str(&call.function.arguments).unwrap_or_else(|_| json!({}));
    match call.function.name.as_str() {
        "calculator" => {
            let expression = args["expression"].as_str().unwrap_or_default().to_string();
            calculator(expression).await
        }
        "project_facts" => {
            let topic = args["topic"].as_str().unwrap_or_default().to_string();
            project_facts(topic).await
        }
        other => Ok(format!("unknown tool: {other}")),
    }
}

// --- The agent loop ---

#[agent]
async fn research_agent(question: String) -> anyhow::Result<String> {
    let client = OpenRouterClient::from_env()?;
    let mut messages = vec![
        ChatMessage::text("system", SYSTEM_PROMPT),
        ChatMessage::text("user", &question),
    ];
    let mut tokens_spent: u64 = 0;

    for step in 1..=MAX_STEPS {
        event(EventKind::Budget, "budget_check")
            .attribute("step", json!(step))
            .attribute("tokens_spent", json!(tokens_spent))
            .attribute(
                "tokens_remaining",
                json!(TOKEN_BUDGET.saturating_sub(tokens_spent)),
            )
            .record()
            .await;
        if tokens_spent >= TOKEN_BUDGET {
            event(EventKind::Termination, "token_budget_exhausted")
                .attribute("tokens_spent", json!(tokens_spent))
                .record()
                .await;
            bail!("token budget of {TOKEN_BUDGET} exhausted after {tokens_spent} tokens");
        }

        let response = client.chat(&messages).await?;
        if let Some(usage) = &response.usage {
            tokens_spent += usage.prompt_tokens + usage.completion_tokens;
        }
        let message = response
            .choices
            .into_iter()
            .next()
            .map(|choice| choice.message)
            .ok_or_else(|| anyhow!("OpenRouter returned no choices"))?;

        let tool_calls = message.tool_calls.clone().unwrap_or_default();
        let content = message.content.clone();
        messages.push(message);

        if tool_calls.is_empty() {
            return content
                .filter(|answer| !answer.is_empty())
                .ok_or_else(|| anyhow!("model returned an empty answer"));
        }
        for call in &tool_calls {
            let output = dispatch_tool_call(call)
                .await
                .unwrap_or_else(|err| format!("tool error: {err}"));
            messages.push(ChatMessage::tool_result(&call.id, output));
        }
    }

    event(EventKind::Termination, "max_steps_reached")
        .attribute("max_steps", json!(MAX_STEPS))
        .record()
        .await;
    bail!("agent did not produce a final answer within {MAX_STEPS} steps")
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_local(LocalConfig {
        database_path: "./.trace-weft/traces.jsonl".into(),
        sqlite_db_path: "./.trace-weft/traces.sqlite".into(),
        blob_dir: "./.trace-weft/blobs".into(),
        // Full local capture so the workbench shows real prompts and tool IO;
        // switch to RedactedPreview when traces may leave your machine.
        capture_content: CapturePolicy::FullContentLocalOnly,
    })
    .await?;

    let question = std::env::args()
        .nth(1)
        .unwrap_or_else(|| DEFAULT_QUESTION.to_string());
    println!("Question: {question}\n");

    let answer = research_agent(question).await?;
    println!("{answer}");
    println!("\nInspect the trace: `trace-weft dev`, then open the web UI.");
    Ok(())
}

// --- A tiny arithmetic evaluator for the calculator tool ---

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Num(f64),
    Plus,
    Minus,
    Star,
    Slash,
    LParen,
    RParen,
}

fn tokenize(input: &str) -> anyhow::Result<Vec<Token>> {
    let mut tokens = Vec::new();
    let mut chars = input.chars().peekable();
    while let Some(&c) = chars.peek() {
        match c {
            ' ' | '\t' | ',' | '_' => {
                chars.next();
            }
            '+' => {
                chars.next();
                tokens.push(Token::Plus);
            }
            '-' => {
                chars.next();
                tokens.push(Token::Minus);
            }
            '*' | 'x' => {
                chars.next();
                tokens.push(Token::Star);
            }
            '/' => {
                chars.next();
                tokens.push(Token::Slash);
            }
            '(' => {
                chars.next();
                tokens.push(Token::LParen);
            }
            ')' => {
                chars.next();
                tokens.push(Token::RParen);
            }
            '0'..='9' | '.' => {
                let mut number = String::new();
                while let Some(&d) = chars.peek() {
                    if d.is_ascii_digit() || d == '.' {
                        number.push(d);
                        chars.next();
                    } else if d == '_' {
                        chars.next();
                    } else {
                        break;
                    }
                }
                tokens.push(Token::Num(number.parse()?));
            }
            other => bail!("unsupported character {other:?} in expression"),
        }
    }
    Ok(tokens)
}

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    fn expr(&mut self) -> anyhow::Result<f64> {
        let mut value = self.term()?;
        while let Some(op) = self.peek().cloned() {
            match op {
                Token::Plus => {
                    self.pos += 1;
                    value += self.term()?;
                }
                Token::Minus => {
                    self.pos += 1;
                    value -= self.term()?;
                }
                _ => break,
            }
        }
        Ok(value)
    }

    fn term(&mut self) -> anyhow::Result<f64> {
        let mut value = self.factor()?;
        while let Some(op) = self.peek().cloned() {
            match op {
                Token::Star => {
                    self.pos += 1;
                    value *= self.factor()?;
                }
                Token::Slash => {
                    self.pos += 1;
                    let divisor = self.factor()?;
                    if divisor == 0.0 {
                        bail!("division by zero");
                    }
                    value /= divisor;
                }
                _ => break,
            }
        }
        Ok(value)
    }

    fn factor(&mut self) -> anyhow::Result<f64> {
        match self.peek().cloned() {
            Some(Token::Num(n)) => {
                self.pos += 1;
                Ok(n)
            }
            Some(Token::Minus) => {
                self.pos += 1;
                Ok(-self.factor()?)
            }
            Some(Token::LParen) => {
                self.pos += 1;
                let value = self.expr()?;
                if self.peek() != Some(&Token::RParen) {
                    bail!("missing closing parenthesis");
                }
                self.pos += 1;
                Ok(value)
            }
            other => bail!("unexpected token {other:?} in expression"),
        }
    }
}

fn eval_expression(input: &str) -> anyhow::Result<f64> {
    let tokens = tokenize(input)?;
    if tokens.is_empty() {
        bail!("empty expression");
    }
    let mut parser = Parser { tokens, pos: 0 };
    let value = parser.expr()?;
    if parser.pos != parser.tokens.len() {
        bail!("unexpected trailing input in expression: {input}");
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::eval_expression;

    #[test]
    fn evaluates_precedence_and_parens() {
        assert_eq!(eval_expression("3 * 12 * (1 + 2)").unwrap(), 108.0);
        assert_eq!(eval_expression("(12000 * 0.75) / 3").unwrap(), 3000.0);
        assert_eq!(eval_expression("-4 + 2").unwrap(), -2.0);
    }

    #[test]
    fn rejects_bad_input() {
        assert!(eval_expression("2 +").is_err());
        assert!(eval_expression("1 / 0").is_err());
        assert!(eval_expression("rm -rf").is_err());
    }
}
