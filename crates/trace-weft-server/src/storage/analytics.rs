use clickhouse::{Client, Row};
use serde::{Deserialize, Serialize};
use trace_weft_core::SpanRecord;

#[derive(Row, Serialize, Deserialize)]
pub struct ClickHouseSpan {
    pub trace_id: String,
    pub span_id: String,
    pub parent_span_id: String,
    pub run_id: String,
    pub span_kind: String,
    pub name: String,
    pub start_time: i64,
    pub end_time: i64,
    pub status: String,
    pub latency_ms: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub total_tokens: i64,
    pub model_provider: String,
    pub model_name: String,
}

impl From<&SpanRecord> for ClickHouseSpan {
    fn from(span: &SpanRecord) -> Self {
        let input_tokens = span
            .token_usage
            .as_ref()
            .map(|t| t.input as i64)
            .unwrap_or(0);
        let output_tokens = span
            .token_usage
            .as_ref()
            .map(|t| t.output as i64)
            .unwrap_or(0);

        Self {
            trace_id: span.trace_id.0.to_string(),
            span_id: span.span_id.0.to_string(),
            parent_span_id: span
                .parent_span_id
                .map(|id| id.0.to_string())
                .unwrap_or_default(),
            run_id: span.run_id.0.to_string(),
            span_kind: format!("{:?}", span.span_kind),
            name: span.name.clone(),
            start_time: span.start_time as i64,
            end_time: span.end_time.unwrap_or(span.start_time) as i64,
            status: format!("{:?}", span.status),
            latency_ms: span.latency_ms.unwrap_or(0) as i64,
            input_tokens,
            output_tokens,
            total_tokens: input_tokens + output_tokens,
            model_provider: span.model_provider.clone().unwrap_or_default(),
            model_name: span.model_name.clone().unwrap_or_default(),
        }
    }
}

pub struct ClickHouseAnalytics {
    client: Client,
}

impl ClickHouseAnalytics {
    pub fn new(url: &str, user: &str, password: &str, database: &str) -> Self {
        let client = Client::default()
            .with_url(url)
            .with_user(user)
            .with_password(password)
            .with_database(database);

        Self { client }
    }

    pub async fn ingest_batch(&self, spans: &[SpanRecord]) -> anyhow::Result<()> {
        let mut insert = self.client.insert("spans_buffer")?;
        for span in spans {
            let ch_span: ClickHouseSpan = span.into();
            insert.write(&ch_span).await?;
        }
        insert.end().await?;
        Ok(())
    }
}
