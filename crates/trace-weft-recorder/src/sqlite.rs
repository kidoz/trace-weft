use crate::TraceStore;
use anyhow::Result;
use sqlx::{SqlitePool, sqlite::SqlitePoolOptions};
use std::path::PathBuf;
use trace_weft_core::{EventRecord, SpanRecord};

pub struct SqliteRecorder {
    pool: SqlitePool,
}

impl SqliteRecorder {
    pub async fn new(db_path: PathBuf) -> Result<Self> {
        if let Some(parent) = db_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        let db_url = format!("sqlite://{}?mode=rwc", db_path.to_string_lossy());

        let pool = SqlitePoolOptions::new().connect(&db_url).await?;

        Self::from_pool(pool).await
    }

    pub async fn from_pool(pool: SqlitePool) -> Result<Self> {
        // Run migrations
        sqlx::migrate!("./migrations").run(&pool).await?;

        Ok(Self { pool })
    }
}

#[async_trait::async_trait]
impl TraceStore for SqliteRecorder {
    async fn record_span(&self, span: SpanRecord) -> Result<()> {
        let trace_id = span.trace_id.0.to_string();
        let span_id = span.span_id.0.to_string();
        let parent_span_id = span.parent_span_id.map(|id| id.0.to_string());
        let run_id = span.run_id.0.to_string();
        let session_id = span.session_id.map(|id| id.0.to_string());
        let span_kind = serde_json::to_string(&span.span_kind)?
            .trim_matches('"')
            .to_string();
        let status = serde_json::to_string(&span.status)?
            .trim_matches('"')
            .to_string();

        let attributes = serde_json::to_string(&span.attributes)?;
        let otel_attributes = serde_json::to_string(&span.otel_attributes)?;
        let openinference_attributes = serde_json::to_string(&span.openinference_attributes)?;
        let memory_state = span
            .memory_state
            .map(|s| serde_json::to_string(&s).unwrap());

        let input_ref = span.input_ref.map(|r| serde_json::to_string(&r).unwrap());
        let output_ref = span.output_ref.map(|r| serde_json::to_string(&r).unwrap());
        let retrieved_document_refs = serde_json::to_string(&span.retrieved_document_refs)?;
        let token_usage = span.token_usage.map(|u| serde_json::to_string(&u).unwrap());
        let cost_estimate = span
            .cost_estimate
            .map(|c| serde_json::to_string(&c).unwrap());
        let redaction_policy = serde_json::to_string(&span.redaction_policy)?
            .trim_matches('"')
            .to_string();

        sqlx::query(
            r#"
            INSERT INTO spans (
                trace_id, span_id, parent_span_id, run_id, session_id, user_id_hash,
                span_kind, name, start_time, end_time, status, status_message, error_type, error_message_redacted,
                attributes, otel_attributes, openinference_attributes, memory_state,
                input_ref, output_ref, prompt_template_id, prompt_version,
                model_provider, model_name, tool_name, tool_schema_hash, retrieval_query_hash,
                retrieved_document_refs, token_usage, cost_estimate, latency_ms, retry_count, cache_hit,
                redaction_policy, schema_version, project_id
            ) VALUES (
                ?, ?, ?, ?, ?, ?,
                ?, ?, ?, ?, ?, ?, ?, ?,
                ?, ?, ?, ?,
                ?, ?, ?, ?,
                ?, ?, ?, ?, ?,
                ?, ?, ?, ?, ?, ?,
                ?, ?, ?
            )
            -- A span may be recorded twice with the same span_id (e.g. a HITL
            -- breakpoint: first PendingApproval, then Ok once resolved). Upsert
            -- so the resolved state replaces the pending row instead of failing
            -- the primary key. For ordinary single-write spans the conflict
            -- arm never fires.
            ON CONFLICT(span_id) DO UPDATE SET
                trace_id=excluded.trace_id, parent_span_id=excluded.parent_span_id,
                run_id=excluded.run_id, session_id=excluded.session_id,
                user_id_hash=excluded.user_id_hash, span_kind=excluded.span_kind,
                name=excluded.name, start_time=excluded.start_time, end_time=excluded.end_time,
                status=excluded.status, status_message=excluded.status_message,
                error_type=excluded.error_type, error_message_redacted=excluded.error_message_redacted,
                attributes=excluded.attributes, otel_attributes=excluded.otel_attributes,
                openinference_attributes=excluded.openinference_attributes,
                memory_state=excluded.memory_state, input_ref=excluded.input_ref,
                output_ref=excluded.output_ref, prompt_template_id=excluded.prompt_template_id,
                prompt_version=excluded.prompt_version, model_provider=excluded.model_provider,
                model_name=excluded.model_name, tool_name=excluded.tool_name,
                tool_schema_hash=excluded.tool_schema_hash,
                retrieval_query_hash=excluded.retrieval_query_hash,
                retrieved_document_refs=excluded.retrieved_document_refs,
                token_usage=excluded.token_usage, cost_estimate=excluded.cost_estimate,
                latency_ms=excluded.latency_ms, retry_count=excluded.retry_count,
                cache_hit=excluded.cache_hit, redaction_policy=excluded.redaction_policy,
                schema_version=excluded.schema_version, project_id=excluded.project_id
            "#,
        )
        .bind(trace_id).bind(span_id).bind(parent_span_id).bind(run_id).bind(session_id).bind(span.user_id_hash)
        .bind(span_kind).bind(span.name).bind(span.start_time as i64).bind(span.end_time.map(|t| t as i64)).bind(status).bind(span.status_message).bind(span.error_type).bind(span.error_message_redacted)
        .bind(attributes).bind(otel_attributes).bind(openinference_attributes).bind(memory_state)
        .bind(input_ref).bind(output_ref).bind(span.prompt_template_id).bind(span.prompt_version)
        .bind(span.model_provider).bind(span.model_name).bind(span.tool_name).bind(span.tool_schema_hash).bind(span.retrieval_query_hash)
        .bind(retrieved_document_refs).bind(token_usage).bind(cost_estimate).bind(span.latency_ms.map(|t| t as i64)).bind(span.retry_count).bind(span.cache_hit)
        .bind(redaction_policy).bind(span.schema_version).bind(span.project_id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn record_event(&self, event: EventRecord) -> Result<()> {
        let event_kind = serde_json::to_string(&event.event_kind)?
            .trim_matches('"')
            .to_string();
        let attributes = serde_json::to_string(&event.attributes)?;

        sqlx::query(
            r#"
            INSERT INTO events (
                event_id, trace_id, run_id, parent_span_id, seq,
                event_kind, name, timestamp, attributes, schema_version
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(event.event_id.0.to_string())
        .bind(event.trace_id.0.to_string())
        .bind(event.run_id.0.to_string())
        .bind(event.parent_span_id.map(|id| id.0.to_string()))
        .bind(event.seq as i64)
        .bind(event_kind)
        .bind(event.name)
        .bind(event.timestamp as i64)
        .bind(attributes)
        .bind(event.schema_version)
        .execute(&self.pool)
        .await?;

        Ok(())
    }
}
