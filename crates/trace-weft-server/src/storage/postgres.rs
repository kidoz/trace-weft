use anyhow::Result;
use sqlx::{PgPool, Postgres, postgres::PgArguments, postgres::PgPoolOptions, query::Query};
use trace_weft_core::SpanRecord;
use trace_weft_recorder::TraceStore;

pub struct PostgresRecorder {
    pub pool: PgPool,
}

impl PostgresRecorder {
    pub async fn new(db_url: &str) -> Result<Self> {
        let pool = PgPoolOptions::new()
            .max_connections(5)
            .connect(db_url)
            .await?;

        // Initialize schema (simplified for demo)
        let q = sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS spans (
                trace_id TEXT NOT NULL,
                span_id TEXT NOT NULL PRIMARY KEY,
                parent_span_id TEXT,
                run_id TEXT NOT NULL,
                session_id TEXT,
                user_id_hash TEXT,
                span_kind TEXT NOT NULL,
                name TEXT NOT NULL,
                start_time BIGINT NOT NULL,
                end_time BIGINT,
                status TEXT NOT NULL,
                status_message TEXT,
                error_type TEXT,
                error_message_redacted TEXT,
                attributes TEXT NOT NULL,
                otel_attributes TEXT NOT NULL,
                openinference_attributes TEXT NOT NULL,
                memory_state TEXT,
                input_ref TEXT,
                output_ref TEXT,
                prompt_template_id TEXT,
                prompt_version TEXT,
                model_provider TEXT,
                model_name TEXT,
                tool_name TEXT,
                tool_schema_hash TEXT,
                retrieval_query_hash TEXT,
                retrieved_document_refs TEXT NOT NULL,
                token_usage TEXT,
                cost_estimate TEXT,
                latency_ms BIGINT,
                retry_count INTEGER,
                cache_hit BOOLEAN,
                redaction_policy TEXT NOT NULL,
                schema_version TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_spans_trace_id ON spans(trace_id);
            CREATE INDEX IF NOT EXISTS idx_spans_run_id ON spans(run_id);
            "#,
        );
        let q: Query<'_, Postgres, PgArguments> = q;
        q.execute(&pool).await?;

        Ok(Self { pool })
    }
}

#[async_trait::async_trait]
impl TraceStore for PostgresRecorder {
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

        let q = sqlx::query(
            r#"
            INSERT INTO spans (
                trace_id, span_id, parent_span_id, run_id, session_id, user_id_hash,
                span_kind, name, start_time, end_time, status, status_message, error_type, error_message_redacted,
                attributes, otel_attributes, openinference_attributes, memory_state,
                input_ref, output_ref, prompt_template_id, prompt_version,
                model_provider, model_name, tool_name, tool_schema_hash, retrieval_query_hash,
                retrieved_document_refs, token_usage, cost_estimate, latency_ms, retry_count, cache_hit,
                redaction_policy, schema_version
            ) VALUES (
                $1, $2, $3, $4, $5, $6,
                $7, $8, $9, $10, $11, $12, $13, $14,
                $15, $16, $17, $18,
                $19, $20, $21, $22,
                $23, $24, $25, $26, $27,
                $28, $29, $30, $31, $32, $33,
                $34, $35
            )
            ON CONFLICT (span_id) DO NOTHING
            "#,
        );

        let q: Query<'_, Postgres, PgArguments> = q;
        q.bind(trace_id)
            .bind(span_id)
            .bind(parent_span_id)
            .bind(run_id)
            .bind(session_id)
            .bind(span.user_id_hash)
            .bind(span_kind)
            .bind(span.name)
            .bind(span.start_time as i64)
            .bind(span.end_time.map(|t| t as i64))
            .bind(status)
            .bind(span.status_message)
            .bind(span.error_type)
            .bind(span.error_message_redacted)
            .bind(attributes)
            .bind(otel_attributes)
            .bind(openinference_attributes)
            .bind(memory_state)
            .bind(input_ref)
            .bind(output_ref)
            .bind(span.prompt_template_id)
            .bind(span.prompt_version)
            .bind(span.model_provider)
            .bind(span.model_name)
            .bind(span.tool_name)
            .bind(span.tool_schema_hash)
            .bind(span.retrieval_query_hash)
            .bind(retrieved_document_refs)
            .bind(token_usage)
            .bind(cost_estimate)
            .bind(span.latency_ms.map(|t| t as i64))
            .bind(span.retry_count.map(|c| c as i32))
            .bind(span.cache_hit)
            .bind(redaction_policy)
            .bind(span.schema_version)
            .execute(&self.pool)
            .await?;

        Ok(())
    }
}
