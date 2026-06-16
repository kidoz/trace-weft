use trace_weft::{SpanBuilder, build_tool};
use trace_weft_core::{SpanId, TraceId};
use uuid::Uuid;

/// A parsed W3C traceparent header or similar propagation context.
#[derive(Debug, Clone)]
pub struct TraceContext {
    pub trace_id: TraceId,
    pub parent_span_id: SpanId,
}

/// Reduce a 128-bit span UUID to the low 8 bytes a W3C span-id carries.
///
/// This is the same reduction `trace-weft-otel` applies when exporting to
/// OTLP, so a span maps to the same 64-bit id on every wire (traceparent and
/// OTLP alike). For the UUIDv7s TraceWeft mints the low bytes are the random
/// `rand_b` field — the high-entropy half — which minimizes sibling collisions.
fn span_id_low_bytes(span_id: SpanId) -> u64 {
    let bytes = span_id.0.as_bytes();
    u64::from_be_bytes(bytes[8..16].try_into().expect("8-byte slice"))
}

fn is_hex(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_hexdigit())
}

impl TraceContext {
    /// Injects the current TraceId and SpanId into a W3C traceparent string.
    /// Example: `00-{trace_id}-{span_id}-01`.
    ///
    /// The trace-id is the full 128-bit UUID (32 hex). The span-id is the low
    /// 64 bits of the span UUID (16 hex) — see [`span_id_low_bytes`].
    pub fn to_traceparent(trace_id: TraceId, span_id: SpanId) -> String {
        let t_hex = trace_id.0.simple().to_string();
        format!("00-{}-{:016x}-01", t_hex, span_id_low_bytes(span_id))
    }

    /// Extracts a TraceContext from a W3C traceparent string.
    ///
    /// Returns `None` for any malformed header: wrong field count, unsupported
    /// version, non-hex segments, wrong segment lengths, or all-zero trace/span
    /// IDs (which W3C defines as invalid). The 64-bit span-id is placed in the
    /// low bytes of the reconstructed UUID, the inverse of [`to_traceparent`].
    pub fn from_traceparent(traceparent: &str) -> Option<Self> {
        let parts: Vec<&str> = traceparent.split('-').collect();
        // version-traceid-spanid-flags
        if parts.len() != 4 {
            return None;
        }
        let (version, trace_str, span_str, flags) = (parts[0], parts[1], parts[2], parts[3]);

        // Only version 00 is defined; "ff" is explicitly forbidden by W3C.
        if version != "00" {
            return None;
        }
        if trace_str.len() != 32 || !is_hex(trace_str) {
            return None;
        }
        if span_str.len() != 16 || !is_hex(span_str) {
            return None;
        }
        if flags.len() != 2 || !is_hex(flags) {
            return None;
        }
        // All-zero trace/span IDs are invalid.
        if trace_str.bytes().all(|b| b == b'0') || span_str.bytes().all(|b| b == b'0') {
            return None;
        }

        let trace_id = Uuid::parse_str(trace_str).ok()?;
        let span_low = u64::from_str_radix(span_str, 16).ok()?;
        let mut span_bytes = [0u8; 16];
        span_bytes[8..16].copy_from_slice(&span_low.to_be_bytes());

        Some(Self {
            trace_id: TraceId(trace_id),
            parent_span_id: SpanId(Uuid::from_bytes(span_bytes)),
        })
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

#[cfg(test)]
mod tests {
    use super::*;
    use trace_weft_core::TraceWeftSpanKind;

    fn trace(n: u128) -> TraceId {
        TraceId(Uuid::from_u128(n))
    }

    /// A span id whose high 8 bytes are zero, so the 64-bit traceparent
    /// reduction round-trips it exactly.
    fn span(n: u64) -> SpanId {
        SpanId(Uuid::from_u128(n as u128))
    }

    #[test]
    fn traceparent_roundtrip_preserves_ids() {
        let t = trace(0x0123_4567_89ab_cdef_0011_2233_4455_6677);
        let s = span(0xdead_beef_cafe_0042);
        let tp = TraceContext::to_traceparent(t, s);
        let parsed = TraceContext::from_traceparent(&tp).expect("valid traceparent");
        assert_eq!(parsed.trace_id, t);
        assert_eq!(parsed.parent_span_id, s);
    }

    #[test]
    fn to_traceparent_emits_wellformed_w3c_shape() {
        let tp = TraceContext::to_traceparent(trace(0x42), span(0x99));
        let parts: Vec<&str> = tp.split('-').collect();
        assert_eq!(parts.len(), 4);
        assert_eq!(parts[0], "00");
        assert_eq!(parts[1].len(), 32);
        assert_eq!(parts[2].len(), 16);
        assert_eq!(parts[3], "01");
        assert!(parts.iter().all(|p| is_hex(p)));
    }

    #[test]
    fn span_id_reduction_matches_otel_low_bytes() {
        // The low 64 bits of the UUID are what both wires carry.
        let s = SpanId(Uuid::from_u128(0x1122_3344_5566_7788_99aa_bbcc_ddee_ff00));
        assert_eq!(span_id_low_bytes(s), 0x99aa_bbcc_ddee_ff00);
    }

    #[test]
    fn rejects_wrong_field_count() {
        assert!(TraceContext::from_traceparent("00-abc-def").is_none());
        let t = format!("{:032x}", 1);
        // Trailing extra field.
        assert!(TraceContext::from_traceparent(&format!("00-{t}-{:016x}-01-extra", 2)).is_none());
    }

    #[test]
    fn rejects_unsupported_version() {
        let t = format!("{:032x}", 1);
        let s = format!("{:016x}", 2);
        assert!(TraceContext::from_traceparent(&format!("ff-{t}-{s}-01")).is_none());
        assert!(TraceContext::from_traceparent(&format!("01-{t}-{s}-01")).is_none());
    }

    #[test]
    fn rejects_non_hex_segments() {
        let s = format!("{:016x}", 2);
        let bad_trace = "zz".repeat(16);
        assert!(TraceContext::from_traceparent(&format!("00-{bad_trace}-{s}-01")).is_none());
        let t = format!("{:032x}", 1);
        let bad_span = "gg".repeat(8);
        assert!(TraceContext::from_traceparent(&format!("00-{t}-{bad_span}-01")).is_none());
    }

    #[test]
    fn rejects_wrong_segment_lengths() {
        let short_trace = format!("{:031x}", 1); // 31 chars
        let s = format!("{:016x}", 2);
        assert!(TraceContext::from_traceparent(&format!("00-{short_trace}-{s}-01")).is_none());
        let t = format!("{:032x}", 1);
        let short_span = format!("{:015x}", 2); // 15 chars
        assert!(TraceContext::from_traceparent(&format!("00-{t}-{short_span}-01")).is_none());
    }

    #[test]
    fn rejects_all_zero_ids() {
        let zero_trace = "0".repeat(32);
        let zero_span = "0".repeat(16);
        let good_trace = format!("{:032x}", 1);
        let good_span = format!("{:016x}", 2);
        assert!(
            TraceContext::from_traceparent(&format!("00-{zero_trace}-{good_span}-01")).is_none()
        );
        assert!(
            TraceContext::from_traceparent(&format!("00-{good_trace}-{zero_span}-01")).is_none()
        );
    }

    #[test]
    fn client_injects_traceparent_carrying_active_span_ids() {
        let mut client = mcp_client("search-server", "tools/call");
        let tp = client.inject_traceparent();

        // Well-formed and parseable.
        let parsed = TraceContext::from_traceparent(&tp).expect("valid traceparent");
        // Carries the active span's trace id and (reduced) span id.
        assert_eq!(parsed.trace_id, client.builder.span.trace_id);
        assert_eq!(
            span_id_low_bytes(parsed.parent_span_id),
            span_id_low_bytes(client.builder.span.span_id)
        );
        assert_eq!(
            TraceContext::to_traceparent(
                client.builder.span.trace_id,
                client.builder.span.span_id
            ),
            tp
        );
        // The injected header is also recorded as an attribute.
        assert_eq!(
            client
                .builder
                .span
                .attributes
                .get("trace_weft.mcp.traceparent.propagated"),
            Some(&serde_json::json!(tp))
        );
    }

    #[test]
    fn client_attaches_mcp_attributes() {
        let client = mcp_client("search-server", "tools/call")
            .request_id("req-1")
            .client_name("agent-x")
            .remote_agent_id("agent-remote");
        let attrs = &client.builder.span.attributes;
        assert_eq!(client.builder.span.span_kind, TraceWeftSpanKind::Handoff);
        assert_eq!(
            attrs.get("trace_weft.mcp.server.name"),
            Some(&serde_json::json!("search-server"))
        );
        assert_eq!(
            attrs.get("trace_weft.mcp.method"),
            Some(&serde_json::json!("tools/call"))
        );
        assert_eq!(
            attrs.get("trace_weft.mcp.request_id"),
            Some(&serde_json::json!("req-1"))
        );
        assert_eq!(
            attrs.get("trace_weft.mcp.client.name"),
            Some(&serde_json::json!("agent-x"))
        );
        assert_eq!(
            attrs.get("trace_weft.handoff.remote_agent_id"),
            Some(&serde_json::json!("agent-remote"))
        );
    }

    #[test]
    fn server_parents_span_to_incoming_traceparent() {
        let t = trace(0xaaaa_bbbb);
        let parent = span(0xc0ff_ee00_1234_5678);
        let tp = TraceContext::to_traceparent(t, parent);

        let server = mcp_server("tools/call", Some(&tp)).server_name("search-server");
        assert_eq!(server.builder.span.trace_id, t);
        assert_eq!(server.builder.span.parent_span_id, Some(parent));
        assert_eq!(
            server.builder.span.attributes.get("trace_weft.mcp.method"),
            Some(&serde_json::json!("tools/call"))
        );
        assert_eq!(
            server
                .builder
                .span
                .attributes
                .get("trace_weft.mcp.server.name"),
            Some(&serde_json::json!("search-server"))
        );
        assert_eq!(
            server
                .builder
                .span
                .attributes
                .get("trace_weft.mcp.traceparent.received"),
            Some(&serde_json::json!(tp))
        );
    }

    #[test]
    fn server_without_traceparent_starts_new_root() {
        let server = mcp_server("tools/call", None);
        assert!(server.builder.span.parent_span_id.is_none());
        assert!(
            !server
                .builder
                .span
                .attributes
                .contains_key("trace_weft.mcp.traceparent.received")
        );
    }

    #[test]
    fn server_invalid_traceparent_starts_new_root_but_keeps_raw_header() {
        let server = mcp_server("tools/call", Some("not-a-valid-traceparent"));
        // Could not parse, so the span stays a root.
        assert!(server.builder.span.parent_span_id.is_none());
        // But the raw header is still recorded for debugging.
        assert_eq!(
            server
                .builder
                .span
                .attributes
                .get("trace_weft.mcp.traceparent.received"),
            Some(&serde_json::json!("not-a-valid-traceparent"))
        );
    }
}
