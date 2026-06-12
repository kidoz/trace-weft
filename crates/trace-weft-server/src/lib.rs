pub mod storage;

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
        .route("/api/traces/:trace_id", get(get_trace))
        .route("/api/evals", get(list_evals))
        .route("/api/v1/batch", post(batch_ingest))
        .route("/api/hitl/pending", get(get_pending_approvals))
        .route("/api/hitl/resolve", post(resolve_approval))
        .layer(CorsLayer::permissive())
        .with_state(state)
}

async fn validate_api_key(headers: &HeaderMap) -> Option<String> {
    // Stub for API key validation returning a Project ID
    if let Some(auth_str) = headers.get("Authorization").and_then(|a| a.to_str().ok())
        && auth_str.starts_with("Bearer tw-")
    {
        return Some("proj_default_123".to_string());
    }
    // In local-first mode or during development without keys, return a default project
    Some("proj_local".to_string())
}

async fn batch_ingest(
    headers: HeaderMap,
    State(state): State<AppState>,
    Json(spans): Json<Vec<SpanRecord>>,
) -> Result<StatusCode, StatusCode> {
    let project_id = match validate_api_key(&headers).await {
        Some(id) => id,
        None => return Err(StatusCode::UNAUTHORIZED),
    };

    tracing::info!(
        "Received batch of {} spans for project {}",
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

async fn list_traces(
    State(state): State<AppState>,
) -> Result<Json<Vec<serde_json::Value>>, StatusCode> {
    let DbPool::Sqlite(pool) = &state.pool else {
        // Not implemented for Postgres yet
        return Err(StatusCode::NOT_IMPLEMENTED);
    };

    let rows = sqlx::query(
        r#"
        SELECT trace_id, run_id, MIN(start_time) as start_time, MAX(end_time) as end_time, 
               COUNT(span_id) as span_count, status
        FROM spans
        GROUP BY trace_id
        ORDER BY start_time DESC
        LIMIT 50
        "#,
    )
    .fetch_all(pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let traces = rows
        .into_iter()
        .map(|row| {
            let trace_id: String = row.get("trace_id");
            let run_id: String = row.get("run_id");
            let start_time: i64 = row.get("start_time");
            let end_time: Option<i64> = row.get("end_time");
            let span_count: i64 = row.get("span_count");
            let status: String = row.get("status");

            serde_json::json!({
                "trace_id": trace_id,
                "run_id": run_id,
                "start_time": start_time,
                "end_time": end_time,
                "span_count": span_count,
                "status": status,
            })
        })
        .collect();

    Ok(Json(traces))
}

async fn list_evals(
    State(state): State<AppState>,
) -> Result<Json<Vec<serde_json::Value>>, StatusCode> {
    let DbPool::Sqlite(pool) = &state.pool else {
        return Err(StatusCode::NOT_IMPLEMENTED);
    };

    let rows = sqlx::query(
        r#"
        SELECT trace_id, span_id, name, start_time, status, attributes
        FROM spans
        WHERE span_kind = 'evaluator' OR span_kind = 'Evaluator'
        ORDER BY start_time DESC
        LIMIT 50
        "#,
    )
    .fetch_all(pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let evals = rows
        .into_iter()
        .map(|row| {
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
                "attributes": serde_json::from_str::<serde_json::Value>(&attributes).unwrap_or(serde_json::json!({})),
            })
        })
        .collect();

    Ok(Json(evals))
}

async fn get_trace(
    Path(trace_id): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<Vec<serde_json::Value>>, StatusCode> {
    let DbPool::Sqlite(pool) = &state.pool else {
        return Err(StatusCode::NOT_IMPLEMENTED);
    };

    let rows = sqlx::query("SELECT * FROM spans WHERE trace_id = ? ORDER BY start_time ASC")
        .bind(trace_id)
        .fetch_all(pool)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut spans = Vec::new();
    for row in rows {
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

        spans.push(serde_json::json!({
            "trace_id": trace_id,
            "span_id": span_id,
            "parent_span_id": parent_span_id,
            "span_kind": span_kind,
            "name": name,
            "start_time": start_time,
            "end_time": end_time,
            "status": status,
            "attributes": serde_json::from_str::<serde_json::Value>(&attributes).unwrap_or(serde_json::json!({})),
            "latency_ms": latency_ms,
            "input_ref": input_ref.and_then(|r| serde_json::from_str::<serde_json::Value>(&r).ok()),
            "output_ref": output_ref.and_then(|r| serde_json::from_str::<serde_json::Value>(&r).ok()),
        }));
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
