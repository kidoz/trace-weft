pub mod auth;
pub mod storage;

use auth::{Auth, AuthConfig};
use axum::{
    Json, Router,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    routing::{get, post},
};
use sqlx::{PgPool, Row, SqlitePool, postgres::PgPoolOptions, sqlite::SqlitePoolOptions};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tower_http::cors::CorsLayer;
use trace_weft_core::SpanRecord;
use trace_weft_recorder::TraceStore;

#[derive(Clone)]
pub enum DbPool {
    Sqlite(SqlitePool),
    Postgres(PgPool),
}

#[derive(Clone)]
pub struct AppState {
    pub pool: DbPool,
    pub blob_store: Arc<dyn trace_weft_core::BlobStore>,
    pub trace_store: Arc<dyn TraceStore>,
    pub clickhouse: Option<Arc<storage::analytics::ClickHouseAnalytics>>,
    pub auth: Arc<AuthConfig>,
}

pub async fn start_server(db_url: &str, port: u16, blob_dir: PathBuf) -> anyhow::Result<()> {
    let pool = if db_url.starts_with("postgres://") || db_url.starts_with("postgresql://") {
        let pg_pool = PgPoolOptions::new().connect(db_url).await?;
        DbPool::Postgres(pg_pool)
    } else {
        // Assume sqlite file path or sqlite:// url
        let url = if db_url.starts_with("sqlite://") {
            db_url.to_string()
        } else {
            if let Some(parent) = std::path::Path::new(db_url).parent() {
                tokio::fs::create_dir_all(parent).await?;
            }
            format!("sqlite://{}?mode=rwc", db_url)
        };
        let sq_pool = SqlitePoolOptions::new().connect(&url).await?;
        DbPool::Sqlite(sq_pool)
    };

    let blob_store = Arc::new(storage::blob::LocalBlobStore::new(blob_dir));

    let trace_store: Arc<dyn TraceStore> = match &pool {
        DbPool::Postgres(pg_pool) => Arc::new(storage::postgres::PostgresRecorder {
            pool: pg_pool.clone(),
        }),
        DbPool::Sqlite(sq_pool) => {
            Arc::new(trace_weft_recorder::sqlite::SqliteRecorder::from_pool(sq_pool.clone()).await?)
        }
    };

    // Enterprise Analytics (Stubbed connection if env var is present)
    let clickhouse = if let Ok(ch_url) = std::env::var("TRACE_WEFT_CH_URL") {
        tracing::info!("Initializing ClickHouse analytics connected to {}", ch_url);
        Some(Arc::new(storage::analytics::ClickHouseAnalytics::new(
            &ch_url, "default", "", "default",
        )))
    } else {
        None
    };

    let state = AppState {
        pool,
        blob_store,
        trace_store,
        clickhouse,
        auth: Arc::new(AuthConfig::from_env()),
    };

    let app = build_router(state);

    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    tracing::info!("Server listening on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

/// Build the TraceWeft API router over the given application state.
pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/api/traces", get(list_traces))
        .route("/api/traces/{trace_id}", get(get_trace))
        .route("/api/evals", get(list_evals))
        .route("/api/v1/batch", post(batch_ingest))
        .route("/api/hitl/pending", get(get_pending_approvals))
        .route("/api/hitl/resolve", post(resolve_approval))
        .layer(CorsLayer::permissive())
        .with_state(state)
}

/// Resolve the request's API key to a tenant, or `401` when none is valid and
/// the dev bypass is off.
fn authorize(state: &AppState, headers: &HeaderMap) -> Result<Auth, StatusCode> {
    state
        .auth
        .authenticate(headers)
        .ok_or(StatusCode::UNAUTHORIZED)
}

async fn batch_ingest(
    headers: HeaderMap,
    State(state): State<AppState>,
    Json(mut spans): Json<Vec<SpanRecord>>,
) -> Result<StatusCode, StatusCode> {
    let auth = authorize(&state, &headers)?;
    // The server is authoritative on tenancy: stamp the authenticated project
    // onto every span so a client cannot assert someone else's project_id.
    let project_id = auth.project().map(|p| p.to_string());
    for span in &mut spans {
        span.project_id = project_id.clone();
    }

    tracing::info!(
        "Received batch of {} spans for project {:?}",
        spans.len(),
        project_id
    );

    // 1. Ingest metadata into Postgres
    for span in &spans {
        // In a real app, this should be a bulk insert
        if let Err(e) = state.trace_store.record_span(span.clone()).await {
            tracing::error!("Failed to record span: {}", e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    }

    // 2. Stream to ClickHouse for analytics
    if let Some(ch) = &state.clickhouse
        && let Err(e) = ch.ingest_batch(&spans).await
    {
        tracing::warn!("Failed to stream to ClickHouse: {}", e);
    }

    Ok(StatusCode::ACCEPTED)
}

/// Log a database error and surface it as a 500. Used by every query handler so
/// failures are recorded rather than silently flattened to an empty body.
fn db_error<E: std::fmt::Display>(e: E) -> StatusCode {
    tracing::error!("database query failed: {e}");
    StatusCode::INTERNAL_SERVER_ERROR
}

/// Decode a JSON column we wrote ourselves. A parse failure means the row is
/// corrupt, so we surface a 500 instead of masking it with an empty object —
/// silently substituting `{}` would hide data loss from the caller.
fn parse_json_column(raw: &str) -> Result<serde_json::Value, StatusCode> {
    serde_json::from_str(raw).map_err(|e| {
        tracing::error!("corrupt JSON in spans column: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })
}

/// Decode a nullable JSON column, preserving SQL `NULL` as JSON `null`.
fn parse_opt_json_column(raw: Option<String>) -> Result<serde_json::Value, StatusCode> {
    match raw {
        Some(s) => parse_json_column(&s),
        None => Ok(serde_json::Value::Null),
    }
}

// The SQLite and Postgres `spans` tables share an identical column layout, so a
// single row shape maps to JSON for either backend. These macros expand the
// same extraction against `SqliteRow` or `PgRow` (the `?` inside propagates to
// the calling handler), keeping the two dialects from drifting apart.

/// One row of the trace-summary aggregate (see `list_traces`).
macro_rules! trace_summary_json {
    ($row:expr) => {{
        let row = $row;
        let trace_id: String = row.get("trace_id");
        let run_id: String = row.get("run_id");
        let start_time: i64 = row.get("start_time");
        let end_time: Option<i64> = row.get("end_time");
        let span_count: i64 = row.get("span_count");
        let has_error: i64 = row.get("has_error");
        serde_json::json!({
            "trace_id": trace_id,
            "run_id": run_id,
            "start_time": start_time,
            "end_time": end_time,
            "span_count": span_count,
            // A trace is errored if any of its spans errored, otherwise ok.
            "status": if has_error != 0 { "error" } else { "ok" },
        })
    }};
}

/// One evaluator span row (see `list_evals`).
macro_rules! eval_row_json {
    ($row:expr) => {{
        let row = $row;
        let trace_id: String = row.get("trace_id");
        let span_id: String = row.get("span_id");
        let name: String = row.get("name");
        let start_time: i64 = row.get("start_time");
        let status: String = row.get("status");
        let attributes: String = row.get("attributes");
        serde_json::json!({
            "trace_id": trace_id,
            "span_id": span_id,
            "name": name,
            "start_time": start_time,
            "status": status,
            "attributes": parse_json_column(&attributes)?,
        })
    }};
}

/// One full span row (see `get_trace`).
macro_rules! span_detail_json {
    ($row:expr) => {{
        let row = $row;
        let trace_id: String = row.get("trace_id");
        let span_id: String = row.get("span_id");
        let parent_span_id: Option<String> = row.get("parent_span_id");
        let span_kind: String = row.get("span_kind");
        let name: String = row.get("name");
        let start_time: i64 = row.get("start_time");
        let end_time: Option<i64> = row.get("end_time");
        let status: String = row.get("status");
        let attributes: String = row.get("attributes");
        let latency_ms: Option<i64> = row.get("latency_ms");
        let input_ref: Option<String> = row.get("input_ref");
        let output_ref: Option<String> = row.get("output_ref");
        serde_json::json!({
            "trace_id": trace_id,
            "span_id": span_id,
            "parent_span_id": parent_span_id,
            "span_kind": span_kind,
            "name": name,
            "start_time": start_time,
            "end_time": end_time,
            "status": status,
            "attributes": parse_json_column(&attributes)?,
            "latency_ms": latency_ms,
            "input_ref": parse_opt_json_column(input_ref)?,
            "output_ref": parse_opt_json_column(output_ref)?,
        })
    }};
}

// Project scoping: each query filters on `project_id` against the bound
// `project` value. A real tenant binds its project id; the dev bypass binds
// SQL `NULL`, and the `OR <param> IS NULL` arm then matches every row so
// local-first runs see all traces. Postgres reuses one `$1`; SQLite repeats the
// positional `?`, so the project value is bound twice there.
//
// The aggregate is portable: every span of a trace shares a run_id (so
// MIN(run_id) is deterministic), the error rollup is CAST to BIGINT so both
// engines decode it as i64, and only grouped/aggregated columns are selected so
// Postgres (which rejects bare columns under GROUP BY) is happy.
const LIST_TRACES_SQL_SQLITE: &str = r#"
    SELECT trace_id, MIN(run_id) AS run_id, MIN(start_time) AS start_time,
           MAX(end_time) AS end_time, COUNT(span_id) AS span_count,
           CAST(MAX(CASE WHEN status = 'error' THEN 1 ELSE 0 END) AS BIGINT) AS has_error
    FROM spans
    WHERE (project_id = ? OR ? IS NULL)
    GROUP BY trace_id
    ORDER BY start_time DESC
    LIMIT 50
"#;

const LIST_TRACES_SQL_PG: &str = r#"
    SELECT trace_id, MIN(run_id) AS run_id, MIN(start_time) AS start_time,
           MAX(end_time) AS end_time, COUNT(span_id) AS span_count,
           CAST(MAX(CASE WHEN status = 'error' THEN 1 ELSE 0 END) AS BIGINT) AS has_error
    FROM spans
    WHERE (project_id = $1 OR $1 IS NULL)
    GROUP BY trace_id
    ORDER BY start_time DESC
    LIMIT 50
"#;

const LIST_EVALS_SQL_SQLITE: &str = r#"
    SELECT trace_id, span_id, name, start_time, status, attributes
    FROM spans
    WHERE (span_kind = 'evaluator' OR span_kind = 'Evaluator')
      AND (project_id = ? OR ? IS NULL)
    ORDER BY start_time DESC
    LIMIT 50
"#;

const LIST_EVALS_SQL_PG: &str = r#"
    SELECT trace_id, span_id, name, start_time, status, attributes
    FROM spans
    WHERE (span_kind = 'evaluator' OR span_kind = 'Evaluator')
      AND (project_id = $1 OR $1 IS NULL)
    ORDER BY start_time DESC
    LIMIT 50
"#;

const GET_TRACE_SQL_SQLITE: &str =
    "SELECT * FROM spans WHERE trace_id = ? AND (project_id = ? OR ? IS NULL) ORDER BY start_time ASC";

const GET_TRACE_SQL_PG: &str =
    "SELECT * FROM spans WHERE trace_id = $1 AND (project_id = $2 OR $2 IS NULL) ORDER BY start_time ASC";

async fn list_traces(
    headers: HeaderMap,
    State(state): State<AppState>,
) -> Result<Json<Vec<serde_json::Value>>, StatusCode> {
    let project = authorize(&state, &headers)?.project().map(str::to_string);
    let mut traces = Vec::new();
    match &state.pool {
        DbPool::Sqlite(pool) => {
            let rows = sqlx::query(LIST_TRACES_SQL_SQLITE)
                .bind(project.clone())
                .bind(project)
                .fetch_all(pool)
                .await
                .map_err(db_error)?;
            for row in &rows {
                traces.push(trace_summary_json!(row));
            }
        }
        DbPool::Postgres(pool) => {
            let rows = sqlx::query(LIST_TRACES_SQL_PG)
                .bind(project)
                .fetch_all(pool)
                .await
                .map_err(db_error)?;
            for row in &rows {
                traces.push(trace_summary_json!(row));
            }
        }
    }
    Ok(Json(traces))
}

async fn list_evals(
    headers: HeaderMap,
    State(state): State<AppState>,
) -> Result<Json<Vec<serde_json::Value>>, StatusCode> {
    let project = authorize(&state, &headers)?.project().map(str::to_string);
    let mut evals = Vec::new();
    match &state.pool {
        DbPool::Sqlite(pool) => {
            let rows = sqlx::query(LIST_EVALS_SQL_SQLITE)
                .bind(project.clone())
                .bind(project)
                .fetch_all(pool)
                .await
                .map_err(db_error)?;
            for row in &rows {
                evals.push(eval_row_json!(row));
            }
        }
        DbPool::Postgres(pool) => {
            let rows = sqlx::query(LIST_EVALS_SQL_PG)
                .bind(project)
                .fetch_all(pool)
                .await
                .map_err(db_error)?;
            for row in &rows {
                evals.push(eval_row_json!(row));
            }
        }
    }
    Ok(Json(evals))
}

async fn get_trace(
    Path(trace_id): Path<String>,
    headers: HeaderMap,
    State(state): State<AppState>,
) -> Result<Json<Vec<serde_json::Value>>, StatusCode> {
    let project = authorize(&state, &headers)?.project().map(str::to_string);
    let mut spans = Vec::new();
    match &state.pool {
        DbPool::Sqlite(pool) => {
            let rows = sqlx::query(GET_TRACE_SQL_SQLITE)
                .bind(trace_id)
                .bind(project.clone())
                .bind(project)
                .fetch_all(pool)
                .await
                .map_err(db_error)?;
            for row in &rows {
                spans.push(span_detail_json!(row));
            }
        }
        DbPool::Postgres(pool) => {
            let rows = sqlx::query(GET_TRACE_SQL_PG)
                .bind(trace_id)
                .bind(project)
                .fetch_all(pool)
                .await
                .map_err(db_error)?;
            for row in &rows {
                spans.push(span_detail_json!(row));
            }
        }
    }
    Ok(Json(spans))
}

use serde::Deserialize;
use trace_weft::hitl::HitlResponse;

async fn get_pending_approvals() -> Result<Json<Vec<String>>, StatusCode> {
    Ok(Json(trace_weft::hitl::get_pending_approvals()))
}

#[derive(Deserialize)]
struct ResolveRequest {
    span_id: String,
    action: String,
    value: Option<serde_json::Value>,
    reason: Option<String>,
}

async fn resolve_approval(Json(req): Json<ResolveRequest>) -> Result<StatusCode, StatusCode> {
    let response = if req.action == "approve" {
        HitlResponse::Approved(req.value.unwrap_or(serde_json::json!({})))
    } else {
        HitlResponse::Rejected(req.reason.unwrap_or_else(|| "Rejected by user".to_string()))
    };

    if trace_weft::hitl::resolve_approval(&req.span_id, response).is_ok() {
        Ok(StatusCode::OK)
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}
