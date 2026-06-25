pub mod auth;
pub mod storage;

use auth::{Auth, AuthConfig};
use axum::{
    Json, Router,
    body::Body,
    extract::{Path, State},
    http::{HeaderMap, HeaderValue, Method, StatusCode, header},
    response::Response,
    routing::{get, post},
};
use sqlx::{PgPool, Row, SqlitePool, postgres::PgPoolOptions, sqlite::SqlitePoolOptions};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tower_http::cors::{AllowOrigin, CorsLayer};
use trace_weft_core::{BlobHash, SpanRecord};
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

/// Start the server with the **production-secure** auth default
/// ([`AuthConfig::from_env`]): unauthenticated requests are rejected unless
/// `TRACE_WEFT_API_KEYS`/`TRACE_WEFT_DEV_MODE` are configured. Runs until the
/// process ends.
pub async fn start_server(db_url: &str, port: u16, blob_dir: PathBuf) -> anyhow::Result<()> {
    start_server_with_shutdown(
        db_url,
        port,
        blob_dir,
        AuthConfig::from_env(),
        std::future::pending::<()>(),
    )
    .await
}

/// Start a **local-first** dev server: the auth bypass defaults on when no keys
/// are configured (see [`AuthConfig::from_env_local_first`]), so the local UI
/// works without keys. Used by `trace-weft dev`.
pub async fn start_dev_server(db_url: &str, port: u16, blob_dir: PathBuf) -> anyhow::Result<()> {
    start_server_with_shutdown(
        db_url,
        port,
        blob_dir,
        AuthConfig::from_env_local_first(),
        std::future::pending::<()>(),
    )
    .await
}

/// Start the server with an explicit [`AuthConfig`], stopping gracefully when
/// `shutdown` resolves. Used by the desktop app to start/stop the embedded
/// server on demand and to drain it cleanly on app exit.
pub async fn start_server_with_shutdown(
    db_url: &str,
    port: u16,
    blob_dir: PathBuf,
    auth: AuthConfig,
    shutdown: impl std::future::Future<Output = ()> + Send + 'static,
) -> anyhow::Result<()> {
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
        auth: Arc::new(auth),
    };

    let app = build_router(state);

    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    tracing::info!("Server listening on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown)
        .await?;

    Ok(())
}

/// Build the TraceWeft API router over the given application state.
pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/api/traces", get(list_traces))
        .route("/api/traces/{trace_id}", get(get_trace))
        .route("/api/traces/{trace_id}/events", get(get_trace_events))
        .route("/api/blobs/{hash}", get(get_blob))
        .route("/api/evals", get(list_evals))
        .route("/api/v1/batch", post(batch_ingest))
        .route("/api/hitl/pending", get(get_pending_approvals))
        .route("/api/hitl/resolve", post(resolve_approval))
        .layer(local_cors())
        .with_state(state)
}

/// CORS for a local-first server: only the local dev UI and the desktop webview
/// may read API responses. A permissive policy would let any website the user
/// visits script `127.0.0.1:<port>` and exfiltrate locally-stored prompts and
/// tool outputs (and, for JSON `POST`s, drive HITL/ingest via CSRF). Restricting
/// the allowed origins makes the browser block both.
fn local_cors() -> CorsLayer {
    CorsLayer::new()
        .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
        .allow_headers([header::AUTHORIZATION, header::CONTENT_TYPE])
        .allow_origin(AllowOrigin::predicate(|origin: &HeaderValue, _req| {
            origin.to_str().map(is_allowed_origin).unwrap_or(false)
        }))
}

/// Allow the Tauri webview origins and loopback (any port, for the Vite dev
/// server and direct browser access); reject everything else.
fn is_allowed_origin(origin: &str) -> bool {
    if origin == "tauri://localhost" || origin == "http://tauri.localhost" {
        return true;
    }
    ["http://localhost", "http://127.0.0.1"]
        .iter()
        .any(|host| origin == *host || origin.starts_with(&format!("{host}:")))
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
        let root_name: Option<String> = row.get("root_name");
        let root_span_kind: Option<String> = row.get("root_span_kind");
        let model_provider: Option<String> = row.get("model_provider");
        let model_name: Option<String> = row.get("model_name");
        let error_summary: Option<String> = row.get("error_summary");
        serde_json::json!({
            "trace_id": trace_id,
            "run_id": run_id,
            "start_time": start_time,
            "end_time": end_time,
            "span_count": span_count,
            "root_name": root_name,
            "root_span_kind": root_span_kind,
            "model_provider": model_provider,
            "model_name": model_name,
            "error_summary": error_summary,
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
        let run_id: String = row.get("run_id");
        let session_id: Option<String> = row.get("session_id");
        let user_id_hash: Option<String> = row.get("user_id_hash");
        let project_id: Option<String> = row.get("project_id");
        let span_kind: String = row.get("span_kind");
        let name: String = row.get("name");
        let start_time: i64 = row.get("start_time");
        let end_time: Option<i64> = row.get("end_time");
        let status: String = row.get("status");
        let status_message: Option<String> = row.get("status_message");
        let error_type: Option<String> = row.get("error_type");
        let error_message_redacted: Option<String> = row.get("error_message_redacted");
        let attributes: String = row.get("attributes");
        let otel_attributes: String = row.get("otel_attributes");
        let openinference_attributes: String = row.get("openinference_attributes");
        let memory_state: Option<String> = row.get("memory_state");
        let latency_ms: Option<i64> = row.get("latency_ms");
        let input_ref: Option<String> = row.get("input_ref");
        let output_ref: Option<String> = row.get("output_ref");
        let prompt_template_id: Option<String> = row.get("prompt_template_id");
        let prompt_version: Option<String> = row.get("prompt_version");
        let model_provider: Option<String> = row.get("model_provider");
        let model_name: Option<String> = row.get("model_name");
        let tool_name: Option<String> = row.get("tool_name");
        let tool_schema_hash: Option<String> = row.get("tool_schema_hash");
        let retrieval_query_hash: Option<String> = row.get("retrieval_query_hash");
        let retrieved_document_refs: String = row.get("retrieved_document_refs");
        let token_usage: Option<String> = row.get("token_usage");
        let cost_estimate: Option<String> = row.get("cost_estimate");
        let retry_count: Option<i64> = row.get("retry_count");
        let cache_hit: Option<bool> = row.get("cache_hit");
        let redaction_policy: String = row.get("redaction_policy");
        let schema_version: String = row.get("schema_version");
        serde_json::json!({
            "trace_id": trace_id,
            "span_id": span_id,
            "parent_span_id": parent_span_id,
            "run_id": run_id,
            "session_id": session_id,
            "user_id_hash": user_id_hash,
            "project_id": project_id,
            "span_kind": span_kind,
            "name": name,
            "start_time": start_time,
            "end_time": end_time,
            "status": status,
            "status_message": status_message,
            "error_type": error_type,
            "error_message_redacted": error_message_redacted,
            "attributes": parse_json_column(&attributes)?,
            "otel_attributes": parse_json_column(&otel_attributes)?,
            "openinference_attributes": parse_json_column(&openinference_attributes)?,
            "memory_state": parse_opt_json_column(memory_state)?,
            "latency_ms": latency_ms,
            "input_ref": parse_opt_json_column(input_ref)?,
            "output_ref": parse_opt_json_column(output_ref)?,
            "prompt_template_id": prompt_template_id,
            "prompt_version": prompt_version,
            "model_provider": model_provider,
            "model_name": model_name,
            "tool_name": tool_name,
            "tool_schema_hash": tool_schema_hash,
            "retrieval_query_hash": retrieval_query_hash,
            "retrieved_document_refs": parse_json_column(&retrieved_document_refs)?,
            "token_usage": parse_opt_json_column(token_usage)?,
            "cost_estimate": parse_opt_json_column(cost_estimate)?,
            "retry_count": retry_count,
            "cache_hit": cache_hit,
            "redaction_policy": redaction_policy,
            "schema_version": schema_version,
        })
    }};
}

/// One event row (see `get_trace_events`).
macro_rules! event_detail_json {
    ($row:expr) => {{
        let row = $row;
        let event_id: String = row.get("event_id");
        let trace_id: String = row.get("trace_id");
        let run_id: String = row.get("run_id");
        let parent_span_id: Option<String> = row.get("parent_span_id");
        let seq: i64 = row.get("seq");
        let event_kind: String = row.get("event_kind");
        let name: String = row.get("name");
        let timestamp: i64 = row.get("timestamp");
        let attributes: String = row.get("attributes");
        let schema_version: String = row.get("schema_version");
        serde_json::json!({
            "event_id": event_id,
            "trace_id": trace_id,
            "run_id": run_id,
            "parent_span_id": parent_span_id,
            "seq": seq,
            "event_kind": event_kind,
            "name": name,
            "timestamp": timestamp,
            "attributes": parse_json_column(&attributes)?,
            "schema_version": schema_version,
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
           CAST(MAX(CASE WHEN status = 'error' THEN 1 ELSE 0 END) AS BIGINT) AS has_error,
           COALESCE(
             MIN(CASE WHEN parent_span_id IS NULL THEN name END),
             MIN(name)
           ) AS root_name,
           COALESCE(
             MIN(CASE WHEN parent_span_id IS NULL THEN span_kind END),
             MIN(span_kind)
           ) AS root_span_kind,
           MAX(model_provider) AS model_provider,
           MAX(model_name) AS model_name,
           MIN(CASE WHEN status = 'error' THEN error_message_redacted END) AS error_summary
    FROM spans
    WHERE (project_id = ? OR ? IS NULL)
    GROUP BY trace_id
    ORDER BY start_time DESC
    LIMIT 50
"#;

const LIST_TRACES_SQL_PG: &str = r#"
    SELECT trace_id, MIN(run_id) AS run_id, MIN(start_time) AS start_time,
           MAX(end_time) AS end_time, COUNT(span_id) AS span_count,
           CAST(MAX(CASE WHEN status = 'error' THEN 1 ELSE 0 END) AS BIGINT) AS has_error,
           COALESCE(
             MIN(CASE WHEN parent_span_id IS NULL THEN name END),
             MIN(name)
           ) AS root_name,
           COALESCE(
             MIN(CASE WHEN parent_span_id IS NULL THEN span_kind END),
             MIN(span_kind)
           ) AS root_span_kind,
           MAX(model_provider) AS model_provider,
           MAX(model_name) AS model_name,
           MIN(CASE WHEN status = 'error' THEN error_message_redacted END) AS error_summary
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

const GET_TRACE_SQL_SQLITE: &str = "SELECT * FROM spans WHERE trace_id = ? AND (project_id = ? OR ? IS NULL) ORDER BY start_time ASC";

const GET_TRACE_SQL_PG: &str = "SELECT * FROM spans WHERE trace_id = $1 AND (project_id = $2 OR $2 IS NULL) ORDER BY start_time ASC";

const GET_TRACE_EVENTS_SQL_SQLITE: &str = r#"
    SELECT e.*
    FROM events e
    WHERE e.trace_id = ?
      AND EXISTS (
        SELECT 1 FROM spans s
        WHERE s.trace_id = e.trace_id
          AND (s.project_id = ? OR ? IS NULL)
      )
    ORDER BY e.timestamp ASC, e.seq ASC
"#;

const GET_TRACE_EVENTS_SQL_PG: &str = r#"
    SELECT e.*
    FROM events e
    WHERE e.trace_id = $1
      AND EXISTS (
        SELECT 1 FROM spans s
        WHERE s.trace_id = e.trace_id
          AND (s.project_id = $2 OR $2 IS NULL)
      )
    ORDER BY e.timestamp ASC, e.seq ASC
"#;

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

async fn get_trace_events(
    Path(trace_id): Path<String>,
    headers: HeaderMap,
    State(state): State<AppState>,
) -> Result<Json<Vec<serde_json::Value>>, StatusCode> {
    let project = authorize(&state, &headers)?.project().map(str::to_string);
    let mut events = Vec::new();
    match &state.pool {
        DbPool::Sqlite(pool) => {
            let rows = sqlx::query(GET_TRACE_EVENTS_SQL_SQLITE)
                .bind(trace_id)
                .bind(project.clone())
                .bind(project)
                .fetch_all(pool)
                .await
                .map_err(db_error)?;
            for row in &rows {
                events.push(event_detail_json!(row));
            }
        }
        DbPool::Postgres(pool) => {
            let rows = sqlx::query(GET_TRACE_EVENTS_SQL_PG)
                .bind(trace_id)
                .bind(project)
                .fetch_all(pool)
                .await
                .map_err(db_error)?;
            for row in &rows {
                events.push(event_detail_json!(row));
            }
        }
    }
    Ok(Json(events))
}

async fn get_blob(
    Path(hash): Path<String>,
    headers: HeaderMap,
    State(state): State<AppState>,
) -> Result<Response<Body>, StatusCode> {
    authorize(&state, &headers)?;
    let hash = BlobHash(hash);
    let Some(bytes) = state.blob_store.get_blob(&hash).await.map_err(db_error)? else {
        return Err(StatusCode::NOT_FOUND);
    };

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/octet-stream")
        .header(header::CACHE_CONTROL, "no-store")
        .body(Body::from(bytes))
        .map_err(|e| {
            tracing::error!("failed to build blob response: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

use serde::Deserialize;
use trace_weft::hitl::HitlResponse;

async fn get_pending_approvals(
    headers: HeaderMap,
    State(state): State<AppState>,
) -> Result<Json<Vec<String>>, StatusCode> {
    authorize(&state, &headers)?;
    Ok(Json(trace_weft::hitl::get_pending_approvals()))
}

#[derive(Deserialize)]
struct ResolveRequest {
    span_id: String,
    action: String,
    value: Option<serde_json::Value>,
    reason: Option<String>,
}

async fn resolve_approval(
    headers: HeaderMap,
    State(state): State<AppState>,
    Json(req): Json<ResolveRequest>,
) -> Result<StatusCode, StatusCode> {
    authorize(&state, &headers)?;
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

#[cfg(test)]
mod cors_tests {
    use super::is_allowed_origin;

    #[test]
    fn allows_local_ui_and_tauri_origins() {
        for origin in [
            "http://localhost:5173",
            "http://127.0.0.1:5173",
            "http://localhost:3000",
            "http://localhost",
            "http://127.0.0.1",
            "tauri://localhost",
            "http://tauri.localhost",
        ] {
            assert!(is_allowed_origin(origin), "{origin} should be allowed");
        }
    }

    #[test]
    fn rejects_external_and_lookalike_origins() {
        for origin in [
            "https://evil.example.com",
            "http://localhost.evil.com",
            "http://127.0.0.1.evil.com",
            "https://localhost:5173",
            "http://evil.com",
            "null",
        ] {
            assert!(!is_allowed_origin(origin), "{origin} should be rejected");
        }
    }
}
