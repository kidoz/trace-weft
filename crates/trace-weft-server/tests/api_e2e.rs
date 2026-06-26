//! End-to-end tests for the HTTP API: spans recorded through the SQLite
//! recorder (or ingested over the batch endpoint) must come back out of the
//! query endpoints.

use std::path::Path;
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use sqlx::sqlite::SqlitePoolOptions;
use tempfile::TempDir;
use tower::ServiceExt;
use trace_weft_core::test_util::{sample_span_full, sample_span_minimal};
use trace_weft_core::{
    EventId, EventKind, EventRecord, SpanId, SpanRecord, SpanStatus, TraceId, TraceWeftSpanKind,
};
use trace_weft_recorder::TraceStore;
use trace_weft_recorder::sqlite::SqliteRecorder;
use trace_weft_server::{AppState, DbPool, build_router};

use trace_weft_server::auth::AuthConfig;

/// Build an app router backed by a fresh SQLite database in `dir` with the dev
/// bypass enabled, returning the router and the recorder that writes into the
/// same database.
async fn test_app(dir: &Path) -> (Router, Arc<SqliteRecorder>) {
    test_app_with_auth(dir, AuthConfig::new(Vec::new(), true)).await
}

/// Like [`test_app`] but with an explicit auth configuration, for exercising
/// rejection and per-project scoping.
async fn test_app_with_auth(dir: &Path, auth: AuthConfig) -> (Router, Arc<SqliteRecorder>) {
    let db_path = dir.join("traces.sqlite");
    let recorder = Arc::new(SqliteRecorder::new(db_path.clone()).await.unwrap());

    let pool = SqlitePoolOptions::new()
        .connect(&format!("sqlite://{}", db_path.to_string_lossy()))
        .await
        .unwrap();

    let state = AppState {
        pool: DbPool::Sqlite(pool),
        blob_store: Arc::new(trace_weft_server::storage::blob::LocalBlobStore::new(
            dir.join("blobs"),
        )),
        trace_store: recorder.clone(),
        clickhouse: None,
        auth: Arc::new(auth),
    };

    (build_router(state), recorder)
}

async fn get_json(app: &Router, uri: &str) -> (StatusCode, serde_json::Value) {
    let response = app
        .clone()
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let value = if bytes.is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap()
    };
    (status, value)
}

async fn get_bytes(app: &Router, uri: &str) -> (StatusCode, Vec<u8>) {
    let response = app
        .clone()
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    (status, bytes.to_vec())
}

/// A root agent span with an LLM child and an evaluator child, all in one trace.
fn sample_trace() -> Vec<SpanRecord> {
    let root = {
        let mut span = sample_span_minimal();
        span.span_kind = TraceWeftSpanKind::Agent;
        span.name = "e2e-root".into();
        span.status = SpanStatus::Ok;
        span.end_time = Some(span.start_time + 100);
        span
    };

    let mut llm = sample_span_full();
    llm.trace_id = root.trace_id;
    llm.run_id = root.run_id;
    llm.parent_span_id = Some(root.span_id);
    llm.span_id = SpanId(uuid::Uuid::from_u128(0x201));
    llm.start_time = root.start_time + 10;

    let mut evaluator = sample_span_minimal();
    evaluator.trace_id = root.trace_id;
    evaluator.run_id = root.run_id;
    evaluator.parent_span_id = Some(root.span_id);
    evaluator.span_id = SpanId(uuid::Uuid::from_u128(0x202));
    evaluator.span_kind = TraceWeftSpanKind::Evaluator;
    evaluator.name = "eval-safety".into();
    evaluator.status = SpanStatus::Ok;
    evaluator.start_time = root.start_time + 20;
    evaluator
        .attributes
        .insert("eval.passed".into(), serde_json::json!(true));

    vec![root, llm, evaluator]
}

fn trace_event(trace_id: TraceId, run_id: trace_weft_core::RunId, parent: SpanId) -> EventRecord {
    let mut attributes = std::collections::HashMap::new();
    attributes.insert("attempt".to_string(), serde_json::json!(2));

    EventRecord {
        event_id: EventId(uuid::Uuid::from_u128(0x404)),
        trace_id,
        run_id,
        parent_span_id: Some(parent),
        seq: 7,
        event_kind: EventKind::Retry,
        name: "retry-after-rate-limit".into(),
        timestamp: 1_715_000_000_055,
        attributes,
        schema_version: "1.0".into(),
    }
}

#[tokio::test]
async fn recorded_trace_is_served_by_query_endpoints() {
    let dir = TempDir::new().unwrap();
    let (app, recorder) = test_app(dir.path()).await;

    let spans = sample_trace();
    let trace_id = spans[0].trace_id.0.to_string();
    for span in &spans {
        recorder.record_span(span.clone()).await.unwrap();
    }
    let event = trace_event(spans[0].trace_id, spans[0].run_id, spans[1].span_id);
    recorder.record_event(event.clone()).await.unwrap();

    // Trace list groups the three spans into one trace.
    let (status, traces) = get_json(&app, "/api/traces").await;
    assert_eq!(status, StatusCode::OK);
    let traces = traces.as_array().unwrap();
    assert_eq!(traces.len(), 1);
    assert_eq!(traces[0]["trace_id"], serde_json::json!(trace_id));
    assert_eq!(traces[0]["span_count"], serde_json::json!(3));
    assert_eq!(traces[0]["root_name"], serde_json::json!("e2e-root"));
    assert_eq!(traces[0]["root_span_kind"], serde_json::json!("agent"));
    assert_eq!(traces[0]["model_provider"], serde_json::json!("openai"));
    assert_eq!(traces[0]["model_name"], serde_json::json!("gpt-4.1"));
    assert_eq!(traces[0]["error_summary"], serde_json::Value::Null);

    // Trace detail returns all spans ordered by start time.
    let (status, detail) = get_json(&app, &format!("/api/traces/{trace_id}")).await;
    assert_eq!(status, StatusCode::OK);
    let detail = detail.as_array().unwrap();
    assert_eq!(detail.len(), 3);
    assert_eq!(detail[0]["name"], serde_json::json!("e2e-root"));
    assert_eq!(detail[0]["parent_span_id"], serde_json::Value::Null);
    assert_eq!(detail[1]["span_kind"], serde_json::json!("llm_call"));
    assert_eq!(
        detail[1]["parent_span_id"],
        serde_json::json!(spans[0].span_id.0.to_string())
    );
    // JSON columns are decoded, not returned as strings.
    assert_eq!(
        detail[1]["attributes"]["custom.key"],
        serde_json::json!("value")
    );
    assert_eq!(
        detail[1]["input_ref"]["content_type"],
        serde_json::json!("text/plain")
    );
    assert_eq!(
        detail[1]["run_id"],
        serde_json::json!(spans[0].run_id.0.to_string())
    );
    assert_eq!(
        detail[1]["session_id"],
        serde_json::json!(spans[1].session_id.unwrap().0.to_string())
    );
    assert_eq!(detail[1]["user_id_hash"], serde_json::json!("user-hash"));
    assert_eq!(detail[1]["status_message"], serde_json::json!("completed"));
    assert_eq!(
        detail[1]["otel_attributes"]["gen_ai.operation.name"],
        serde_json::json!("chat")
    );
    assert_eq!(
        detail[1]["openinference_attributes"]["llm.system"],
        serde_json::json!("openai")
    );
    assert_eq!(
        detail[1]["memory_state"]["scratchpad"],
        serde_json::json!("notes")
    );
    assert_eq!(
        detail[1]["prompt_template_id"],
        serde_json::json!("support-answer")
    );
    assert_eq!(detail[1]["prompt_version"], serde_json::json!("v7"));
    assert_eq!(detail[1]["model_provider"], serde_json::json!("openai"));
    assert_eq!(detail[1]["model_name"], serde_json::json!("gpt-4.1"));
    assert_eq!(detail[1]["tool_name"], serde_json::json!("kb_search"));
    assert_eq!(
        detail[1]["retrieval_query_hash"],
        serde_json::json!("query-hash")
    );
    assert_eq!(
        detail[1]["retrieved_document_refs"][0]["content_type"],
        "text/plain"
    );
    assert_eq!(detail[1]["token_usage"]["input"], serde_json::json!(100));
    assert_eq!(
        detail[1]["cost_estimate"]["amount"],
        serde_json::json!(0.0123)
    );
    assert_eq!(detail[1]["retry_count"], serde_json::json!(1));
    assert_eq!(detail[1]["cache_hit"], serde_json::json!(false));
    assert_eq!(
        detail[1]["redaction_policy"],
        serde_json::json!("redacted_preview")
    );
    assert_eq!(detail[1]["schema_version"], serde_json::json!("1.0"));

    // Events are available separately and ordered for timeline/transcript views.
    let (status, events) = get_json(&app, &format!("/api/traces/{trace_id}/events")).await;
    assert_eq!(status, StatusCode::OK);
    let events = events.as_array().unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(
        events[0]["event_id"],
        serde_json::json!(event.event_id.0.to_string())
    );
    assert_eq!(events[0]["trace_id"], serde_json::json!(trace_id));
    assert_eq!(
        events[0]["parent_span_id"],
        serde_json::json!(spans[1].span_id.0.to_string())
    );
    assert_eq!(events[0]["seq"], serde_json::json!(7));
    assert_eq!(events[0]["event_kind"], serde_json::json!("retry"));
    assert_eq!(events[0]["attributes"]["attempt"], serde_json::json!(2));

    // Eval listing returns only evaluator spans, with parsed attributes.
    let (status, evals) = get_json(&app, "/api/evals").await;
    assert_eq!(status, StatusCode::OK);
    let evals = evals.as_array().unwrap();
    assert_eq!(evals.len(), 1);
    assert_eq!(evals[0]["name"], serde_json::json!("eval-safety"));
    assert_eq!(
        evals[0]["attributes"]["eval.passed"],
        serde_json::json!(true)
    );
}

#[tokio::test]
async fn empty_database_returns_empty_lists() {
    let dir = TempDir::new().unwrap();
    let (app, _recorder) = test_app(dir.path()).await;

    let (status, traces) = get_json(&app, "/api/traces").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(traces, serde_json::json!([]));

    let (status, detail) = get_json(&app, "/api/traces/no-such-trace").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(detail, serde_json::json!([]));

    let (status, events) = get_json(&app, "/api/traces/no-such-trace/events").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(events, serde_json::json!([]));

    let (status, evals) = get_json(&app, "/api/evals").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(evals, serde_json::json!([]));
}

#[tokio::test]
async fn blob_endpoint_serves_local_blob_bytes() {
    let dir = TempDir::new().unwrap();
    let (app, _recorder) = test_app(dir.path()).await;
    let blob_dir = dir.path().join("blobs");
    tokio::fs::create_dir_all(&blob_dir).await.unwrap();
    tokio::fs::write(blob_dir.join("sha256_local_blob"), b"redacted blob")
        .await
        .unwrap();

    let (status, bytes) = get_bytes(&app, "/api/blobs/sha256:local_blob").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(bytes, b"redacted blob");

    let (status, _) = get_bytes(&app, "/api/blobs/sha256:missing").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn batch_ingest_persists_spans() {
    let dir = TempDir::new().unwrap();
    let (app, _recorder) = test_app(dir.path()).await;

    let spans = sample_trace();
    let trace_id = spans[0].trace_id.0.to_string();

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/batch")
                .header("Content-Type", "application/json")
                .header("Authorization", "Bearer tw-test-key")
                .body(Body::from(serde_json::to_vec(&spans).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::ACCEPTED);

    let (status, detail) = get_json(&app, &format!("/api/traces/{trace_id}")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(detail.as_array().unwrap().len(), 3);
}

/// A minimal OTLP/HTTP JSON export with a single root span.
fn otlp_payload() -> &'static str {
    r#"{
        "resourceSpans": [{
            "scopeSpans": [{
                "spans": [{
                    "traceId": "0123456789abcdef0011223344556677",
                    "spanId": "8899aabbccddeeff",
                    "name": "otlp-root",
                    "startTimeUnixNano": "1715000000000000000",
                    "endTimeUnixNano": "1715000005000000000"
                }]
            }]
        }]
    }"#
}

async fn post_otlp(app: &Router, body: &str, key: Option<&str>) -> StatusCode {
    let mut builder = Request::builder()
        .method("POST")
        .uri("/v1/traces")
        .header("Content-Type", "application/json");
    if let Some(key) = key {
        builder = builder.header("Authorization", format!("Bearer {key}"));
    }
    app.clone()
        .oneshot(builder.body(Body::from(body.to_string())).unwrap())
        .await
        .unwrap()
        .status()
}

#[tokio::test]
async fn otlp_ingest_persists_and_scopes_spans() {
    let dir = TempDir::new().unwrap();
    let auth = AuthConfig::new(
        vec![
            ("tw-alpha".to_string(), "proj_a".to_string()),
            ("tw-beta".to_string(), "proj_b".to_string()),
        ],
        false,
    );
    let (app, _recorder) = test_app_with_auth(dir.path(), auth).await;

    // Unauthenticated OTLP ingest is rejected, and a malformed body is a 400.
    assert_eq!(
        post_otlp(&app, otlp_payload(), None).await,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        post_otlp(&app, "not json", Some("tw-alpha")).await,
        StatusCode::BAD_REQUEST
    );

    // Alpha ingests an OTLP export; the server stamps proj_a onto the spans.
    assert_eq!(
        post_otlp(&app, otlp_payload(), Some("tw-alpha")).await,
        StatusCode::OK
    );

    // Alpha sees the ingested trace; beta (a different project) does not.
    let (status, traces) = get_json_auth(&app, "/api/traces", Some("tw-alpha")).await;
    assert_eq!(status, StatusCode::OK);
    let traces = traces.as_array().unwrap();
    assert_eq!(traces.len(), 1);
    assert_eq!(traces[0]["root_name"], serde_json::json!("otlp-root"));
    assert_eq!(traces[0]["span_count"], serde_json::json!(1));

    let (status, beta_traces) = get_json_auth(&app, "/api/traces", Some("tw-beta")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(beta_traces.as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn openapi_diff_and_replay_endpoints_are_served() {
    let dir = TempDir::new().unwrap();
    let (app, recorder) = test_app(dir.path()).await;

    let mut original = trace_with_seed(0x0c00);
    let mut replayed = trace_with_seed(0x0d00);
    original[0].name = "workflow".into();
    replayed[0].name = "workflow".into();
    original[0].latency_ms = Some(100);
    replayed[0].latency_ms = Some(250);

    for span in original.iter().chain(replayed.iter()) {
        recorder.record_span(span.clone()).await.unwrap();
    }

    let (status, contract) = get_json(&app, "/api/openapi.json").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(contract["openapi"], serde_json::json!("3.1.0"));

    let original_trace = original[0].trace_id.0.to_string();
    let replayed_trace = replayed[0].trace_id.0.to_string();
    let (status, diff) = get_json(
        &app,
        &format!("/api/diff/{original_trace}/{replayed_trace}"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(diff["trace_a"], serde_json::json!(original_trace));
    assert!(
        diff["summary"]["changed"].as_i64().unwrap() >= 1,
        "latency change should be detected: {diff}"
    );

    let span_id = original[0].span_id.0.to_string();
    let (status, plan) = get_json(
        &app,
        &format!("/api/traces/{original_trace}/replay-plan/{span_id}"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(plan["trace_id"], serde_json::json!(original_trace));
    assert_eq!(plan["target_span"]["span_id"], serde_json::json!(span_id));

    let body = serde_json::json!({
        "span_id": span_id,
        "span_name": "workflow",
        "mocked_output": {"ok": true},
        "block_side_effects": true
    });
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/replay/config")
                .header("Content-Type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let config: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(
        config["config"]["block_side_effects"],
        serde_json::json!(true)
    );
    assert_eq!(
        config["config"]["mocked_span_ids"].get(&span_id),
        Some(&serde_json::json!({"ok": true}))
    );
}

#[tokio::test]
async fn hitl_endpoints_resolve_pending_approvals() {
    let dir = TempDir::new().unwrap();
    let (app, _recorder) = test_app(dir.path()).await;

    let span_id = "e2e-server-hitl-span";
    let rx = trace_weft::register_approval(span_id.to_string());

    let (status, pending) = get_json(&app, "/api/hitl/pending").await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        pending
            .as_array()
            .unwrap()
            .contains(&serde_json::json!(span_id))
    );

    let body = serde_json::json!({
        "span_id": span_id,
        "action": "approve",
        "value": {"args": "edited"},
    });
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/hitl/resolve")
                .header("Content-Type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    match rx.await.unwrap() {
        trace_weft::HitlResponse::Approved(value) => {
            assert_eq!(value, serde_json::json!({"args": "edited"}));
        }
        trace_weft::HitlResponse::Rejected(reason) => {
            panic!("expected approval, got rejection: {reason}");
        }
    }
}

/// A distinct trace whose ids derive from `seed`, so independent traces can be
/// ingested without span-id primary-key collisions.
fn trace_with_seed(seed: u128) -> Vec<SpanRecord> {
    let mut spans = sample_trace();
    let trace_id = TraceId(uuid::Uuid::from_u128(seed));
    for (i, span) in spans.iter_mut().enumerate() {
        span.trace_id = trace_id;
        span.span_id = SpanId(uuid::Uuid::from_u128(seed + 100 + i as u128));
    }
    let root_id = spans[0].span_id;
    spans[0].parent_span_id = None;
    for span in spans.iter_mut().skip(1) {
        span.parent_span_id = Some(root_id);
    }
    spans
}

async fn post_batch(app: &Router, spans: &[SpanRecord], key: Option<&str>) -> StatusCode {
    let mut builder = Request::builder()
        .method("POST")
        .uri("/api/v1/batch")
        .header("Content-Type", "application/json");
    if let Some(key) = key {
        builder = builder.header("Authorization", format!("Bearer {key}"));
    }
    app.clone()
        .oneshot(
            builder
                .body(Body::from(serde_json::to_vec(spans).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap()
        .status()
}

async fn get_json_auth(
    app: &Router,
    uri: &str,
    key: Option<&str>,
) -> (StatusCode, serde_json::Value) {
    let mut builder = Request::builder().uri(uri);
    if let Some(key) = key {
        builder = builder.header("Authorization", format!("Bearer {key}"));
    }
    let response = app
        .clone()
        .oneshot(builder.body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let value = if bytes.is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap()
    };
    (status, value)
}

#[tokio::test]
async fn queries_without_a_valid_key_are_rejected_outside_dev_mode() {
    let dir = TempDir::new().unwrap();
    let auth = AuthConfig::new(vec![("tw-secret".to_string(), "proj_a".to_string())], false);
    let (app, _recorder) = test_app_with_auth(dir.path(), auth).await;

    // No header and an unknown key are both rejected.
    let (status, _) = get_json_auth(&app, "/api/traces", None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, _) = get_json_auth(&app, "/api/traces", Some("tw-wrong")).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    // A recognized key is accepted.
    let (status, _) = get_json_auth(&app, "/api/traces", Some("tw-secret")).await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn hitl_endpoints_require_auth_outside_dev_mode() {
    let dir = TempDir::new().unwrap();
    let auth = AuthConfig::new(vec![("tw-secret".to_string(), "proj_a".to_string())], false);
    let (app, _recorder) = test_app_with_auth(dir.path(), auth).await;

    // Pending list is gated.
    let (status, _) = get_json_auth(&app, "/api/hitl/pending", None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    // Resolve is gated too (no auth header → 401, not NOT_FOUND).
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/hitl/resolve")
                .header("Content-Type", "application/json")
                .body(Body::from(
                    serde_json::json!({"span_id": "x", "action": "reject"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn trace_queries_are_scoped_to_the_authenticated_project() {
    let dir = TempDir::new().unwrap();
    let auth = AuthConfig::new(
        vec![
            ("tw-alpha".to_string(), "proj_a".to_string()),
            ("tw-beta".to_string(), "proj_b".to_string()),
        ],
        false,
    );
    let (app, recorder) = test_app_with_auth(dir.path(), auth).await;

    // Each tenant ingests its own trace; the server stamps project_id from the key.
    let alpha = trace_with_seed(0x0a00);
    let beta = trace_with_seed(0x0b00);
    assert_eq!(
        post_batch(&app, &alpha, Some("tw-alpha")).await,
        StatusCode::ACCEPTED
    );
    assert_eq!(
        post_batch(&app, &beta, Some("tw-beta")).await,
        StatusCode::ACCEPTED
    );
    recorder
        .record_event(trace_event(
            alpha[0].trace_id,
            alpha[0].run_id,
            alpha[1].span_id,
        ))
        .await
        .unwrap();
    let mut beta_event = trace_event(beta[0].trace_id, beta[0].run_id, beta[1].span_id);
    beta_event.event_id = EventId(uuid::Uuid::from_u128(0x405));
    recorder.record_event(beta_event).await.unwrap();

    let alpha_trace = alpha[0].trace_id.0.to_string();
    let beta_trace = beta[0].trace_id.0.to_string();

    // Alpha sees only its own trace in the list.
    let (status, traces) = get_json_auth(&app, "/api/traces", Some("tw-alpha")).await;
    assert_eq!(status, StatusCode::OK);
    let traces = traces.as_array().unwrap();
    assert_eq!(traces.len(), 1);
    assert_eq!(traces[0]["trace_id"], serde_json::json!(alpha_trace));

    // And cannot read beta's trace by id (scoped out → empty).
    let (status, detail) =
        get_json_auth(&app, &format!("/api/traces/{beta_trace}"), Some("tw-alpha")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(detail.as_array().unwrap().len(), 0);

    let (status, events) = get_json_auth(
        &app,
        &format!("/api/traces/{beta_trace}/events"),
        Some("tw-alpha"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(events.as_array().unwrap().len(), 0);

    // Beta sees its own trace and not alpha's.
    let (status, traces) = get_json_auth(&app, "/api/traces", Some("tw-beta")).await;
    assert_eq!(status, StatusCode::OK);
    let traces = traces.as_array().unwrap();
    assert_eq!(traces.len(), 1);
    assert_eq!(traces[0]["trace_id"], serde_json::json!(beta_trace));

    let (status, events) = get_json_auth(
        &app,
        &format!("/api/traces/{beta_trace}/events"),
        Some("tw-beta"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(events.as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn server_starts_serves_and_shuts_down_gracefully() {
    use std::time::Duration;

    let dir = TempDir::new().unwrap();
    let db = dir
        .path()
        .join("traces.sqlite")
        .to_string_lossy()
        .into_owned();
    let blob_dir = dir.path().join("blobs");

    // Grab a free port, then release it so the server can bind it.
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();

    let (tx, rx) = tokio::sync::oneshot::channel::<()>();
    let server = tokio::spawn(async move {
        trace_weft_server::start_server_with_shutdown(
            &db,
            port,
            blob_dir,
            AuthConfig::new(Vec::new(), true),
            async move {
                let _ = rx.await;
            },
        )
        .await
    });

    // Wait for it to bind, then confirm it serves a request.
    tokio::time::sleep(Duration::from_millis(300)).await;
    let body = tokio::net::TcpStream::connect(("127.0.0.1", port)).await;
    assert!(body.is_ok(), "server should be accepting connections");

    // Signal shutdown; the server future must resolve promptly.
    tx.send(()).unwrap();
    let result = tokio::time::timeout(Duration::from_secs(5), server).await;
    assert!(
        matches!(result, Ok(Ok(Ok(())))),
        "server should shut down gracefully, got {result:?}"
    );
}

#[tokio::test]
async fn resolving_unknown_approval_returns_not_found() {
    let dir = TempDir::new().unwrap();
    let (app, _recorder) = test_app(dir.path()).await;

    let body = serde_json::json!({
        "span_id": "no-such-span",
        "action": "reject",
        "reason": "nope",
    });
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/hitl/resolve")
                .header("Content-Type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}
