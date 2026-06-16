//! Postgres-backed parity tests for the query endpoints.
//!
//! These are gated behind the `TRACE_WEFT_PG_TEST` environment variable so CI
//! and local runs without a Postgres instance still pass — the test returns
//! early when it is unset. To run it:
//!
//! ```sh
//! TRACE_WEFT_PG_TEST=1 \
//! TRACE_WEFT_PG_URL=postgres://postgres:postgres@localhost:5432/trace_weft_test \
//!   cargo test -p trace-weft-server --test postgres_e2e
//! ```

use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use trace_weft_core::test_util::{sample_span_full, sample_span_minimal};
use trace_weft_core::{SpanId, SpanRecord, SpanStatus, TraceWeftSpanKind};
use trace_weft_recorder::TraceStore;
use trace_weft_server::storage::postgres::PostgresRecorder;
use trace_weft_server::{AppState, DbPool, build_router};

/// Same three-span trace shape used by the SQLite suite, so the Postgres output
/// can be checked against the same expectations.
fn sample_trace() -> Vec<SpanRecord> {
    let root = {
        let mut span = sample_span_minimal();
        span.span_kind = TraceWeftSpanKind::Agent;
        span.name = "pg-e2e-root".into();
        span.status = SpanStatus::Ok;
        span.end_time = Some(span.start_time + 100);
        span
    };

    let mut llm = sample_span_full();
    llm.trace_id = root.trace_id;
    llm.run_id = root.run_id;
    llm.parent_span_id = Some(root.span_id);
    llm.span_id = SpanId(uuid::Uuid::from_u128(0x301));
    llm.start_time = root.start_time + 10;

    let mut evaluator = sample_span_minimal();
    evaluator.trace_id = root.trace_id;
    evaluator.run_id = root.run_id;
    evaluator.parent_span_id = Some(root.span_id);
    evaluator.span_id = SpanId(uuid::Uuid::from_u128(0x302));
    evaluator.span_kind = TraceWeftSpanKind::Evaluator;
    evaluator.name = "pg-eval-safety".into();
    evaluator.status = SpanStatus::Ok;
    evaluator.start_time = root.start_time + 20;
    evaluator
        .attributes
        .insert("eval.passed".into(), serde_json::json!(true));

    vec![root, llm, evaluator]
}

async fn get_json(app: &Router, uri: &str) -> (StatusCode, serde_json::Value) {
    use tower::ServiceExt;
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

#[tokio::test]
async fn postgres_query_endpoints_match_sqlite_shape() {
    if std::env::var("TRACE_WEFT_PG_TEST").is_err() {
        eprintln!("skipping: set TRACE_WEFT_PG_TEST=1 (and TRACE_WEFT_PG_URL) to run");
        return;
    }
    let url = std::env::var("TRACE_WEFT_PG_URL").unwrap_or_else(|_| {
        "postgres://postgres:postgres@localhost:5432/trace_weft_test".to_string()
    });

    // Creates the schema on first connect.
    let recorder = PostgresRecorder::new(&url)
        .await
        .expect("connect to Postgres");
    let pool = recorder.pool.clone();
    // Start from a clean table so assertions are deterministic across runs.
    sqlx::query("DELETE FROM spans")
        .execute(&pool)
        .await
        .expect("clear spans");

    let spans = sample_trace();
    let trace_id = spans[0].trace_id.0.to_string();
    let root_span_id = spans[0].span_id.0.to_string();
    for span in &spans {
        recorder.record_span(span.clone()).await.unwrap();
    }

    let state = AppState {
        pool: DbPool::Postgres(pool),
        blob_store: Arc::new(trace_weft_server::storage::blob::LocalBlobStore::new(
            std::env::temp_dir().join("trace-weft-pg-test-blobs"),
        )),
        trace_store: Arc::new(recorder),
        clickhouse: None,
    };
    let app = build_router(state);

    // list_traces groups the three spans into one trace.
    let (status, traces) = get_json(&app, "/api/traces").await;
    assert_eq!(status, StatusCode::OK);
    let traces = traces.as_array().unwrap();
    assert_eq!(traces.len(), 1);
    assert_eq!(traces[0]["trace_id"], serde_json::json!(trace_id));
    assert_eq!(traces[0]["span_count"], serde_json::json!(3));
    assert_eq!(traces[0]["status"], serde_json::json!("ok"));

    // get_trace returns all spans ordered by start time, with decoded JSON.
    let (status, detail) = get_json(&app, &format!("/api/traces/{trace_id}")).await;
    assert_eq!(status, StatusCode::OK);
    let detail = detail.as_array().unwrap();
    assert_eq!(detail.len(), 3);
    assert_eq!(detail[0]["name"], serde_json::json!("pg-e2e-root"));
    assert_eq!(detail[0]["parent_span_id"], serde_json::Value::Null);
    assert_eq!(detail[1]["span_kind"], serde_json::json!("llm_call"));
    assert_eq!(detail[1]["parent_span_id"], serde_json::json!(root_span_id));
    assert_eq!(
        detail[1]["attributes"]["custom.key"],
        serde_json::json!("value")
    );
    assert_eq!(
        detail[1]["input_ref"]["content_type"],
        serde_json::json!("text/plain")
    );

    // list_evals returns only evaluator spans with parsed attributes.
    let (status, evals) = get_json(&app, "/api/evals").await;
    assert_eq!(status, StatusCode::OK);
    let evals = evals.as_array().unwrap();
    assert_eq!(evals.len(), 1);
    assert_eq!(evals[0]["name"], serde_json::json!("pg-eval-safety"));
    assert_eq!(
        evals[0]["attributes"]["eval.passed"],
        serde_json::json!(true)
    );
}
