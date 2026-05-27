use trace_weft::{SpanBuilder, build_tool};
use trace_weft_core::{SpanId, TraceId};
use uuid::Uuid;

/// A parsed W3C traceparent header or similar propagation context.
#[derive(Debug, Clone)]
pub struct TraceContext {
    pub trace_id: TraceId,
    pub parent_span_id: SpanId,
}

impl TraceContext {
    /// Injects the current TraceId and SpanId into a W3C traceparent string.
    /// Example: 00-{trace_id}-{span_id}-01
    pub fn to_traceparent(trace_id: TraceId, span_id: SpanId) -> String {
        let t_hex = trace_id.0.simple().to_string();
        let s_hex = span_id.0.simple().to_string();
        // W3C span-id is 16 chars. We take the first 16 chars of the UUID hex.
        let s_hex_truncated = &s_hex[0..16];
        format!("00-{}-{}-01", t_hex, s_hex_truncated)
    }

    /// Extracts a TraceContext from a W3C traceparent string.
    /// If the traceparent is malformed or invalid, it returns None.
    pub fn from_traceparent(traceparent: &str) -> Option<Self> {
        let parts: Vec<&str> = traceparent.split('-').collect();
        if parts.len() >= 3 && parts[0] == "00" {
            let trace_id_str = parts[1];
            let span_id_str = parts[2];

            let trace_id = Uuid::parse_str(trace_id_str).ok()?;

            // Reconstruct a full UUID from the 16-char span_id (padding with 0s)
            let padded_span_id = format!("{:0<32}", span_id_str);
            let span_id = Uuid::parse_str(&padded_span_id).ok()?;

            return Some(Self {
                trace_id: TraceId(trace_id),
                parent_span_id: SpanId(span_id),
            });
        }
        None
    }
}

pub struct McpClientBuilder {
    builder: SpanBuilder,
}

impl McpClientBuilder {
    pub fn new(server_name: &str, method: &str) -> Self {
        let mut builder = build_tool(format!("mcp_call: {}", method));
        builder.span.span_kind = trace_weft_core::TraceWeftSpanKind::Handoff;
        builder.span.attributes.insert(
            "trace_weft.mcp.server.name".into(),
            serde_json::json!(server_name),
        );
        builder
            .span
            .attributes
            .insert("trace_weft.mcp.method".into(), serde_json::json!(method));

        Self { builder }
    }

    pub fn request_id(mut self, id: &str) -> Self {
        self.builder
            .span
            .attributes
            .insert("trace_weft.mcp.request_id".into(), serde_json::json!(id));
        self
    }

    pub fn client_name(mut self, name: &str) -> Self {
        self.builder
            .span
            .attributes
            .insert("trace_weft.mcp.client.name".into(), serde_json::json!(name));
        self
    }

    pub fn remote_agent_id(mut self, agent_id: &str) -> Self {
        self.builder.span.attributes.insert(
            "trace_weft.handoff.remote_agent_id".into(),
            serde_json::json!(agent_id),
        );
        self
    }

    /// Generates a traceparent header string to pass to the MCP server.
    pub fn inject_traceparent(&mut self) -> String {
        let traceparent =
            TraceContext::to_traceparent(self.builder.span.trace_id, self.builder.span.span_id);
        self.builder.span.attributes.insert(
            "trace_weft.mcp.traceparent.propagated".into(),
            serde_json::json!(traceparent.clone()),
        );
        traceparent
    }

    pub async fn run<F, Fut, T, E>(self, f: F) -> Result<T, E>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<T, E>>,
        E: std::fmt::Debug + std::fmt::Display + 'static,
        T: serde::de::DeserializeOwned,
    {
        self.builder.run(f).await
    }
}

pub struct McpServerBuilder {
    builder: SpanBuilder,
}

impl McpServerBuilder {
    pub fn new(method: &str, traceparent: Option<&str>) -> Self {
        let mut builder = build_tool(format!("mcp_handle: {}", method));
        builder.span.span_kind = trace_weft_core::TraceWeftSpanKind::Handoff;
        builder
            .span
            .attributes
            .insert("trace_weft.mcp.method".into(), serde_json::json!(method));

        if let Some(tp) = traceparent {
            builder.span.attributes.insert(
                "trace_weft.mcp.traceparent.received".into(),
                serde_json::json!(tp),
            );
            if let Some(ctx) = TraceContext::from_traceparent(tp) {
                // Link this span to the remote trace and parent span
                let run_id = builder.span.run_id;
                builder = builder.with_parent(ctx.trace_id, run_id, ctx.parent_span_id);
            }
        }

        Self { builder }
    }

    pub fn server_name(mut self, name: &str) -> Self {
        self.builder
            .span
            .attributes
            .insert("trace_weft.mcp.server.name".into(), serde_json::json!(name));
        self
    }

    pub async fn run<F, Fut, T, E>(self, f: F) -> Result<T, E>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<T, E>>,
        E: std::fmt::Debug + std::fmt::Display + 'static,
        T: serde::de::DeserializeOwned,
    {
        self.builder.run(f).await
    }
}

pub fn mcp_client(server_name: &str, method: &str) -> McpClientBuilder {
    McpClientBuilder::new(server_name, method)
}

pub fn mcp_server(method: &str, traceparent: Option<&str>) -> McpServerBuilder {
    McpServerBuilder::new(method, traceparent)
}
