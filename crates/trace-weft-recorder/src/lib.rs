use anyhow::Result;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::fs::{self, OpenOptions};
use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex;
use trace_weft_core::{CapturePolicy, EventRecord, SpanRecord};

pub mod sqlite;
use sqlite::SqliteRecorder;

#[derive(Debug, Clone)]
pub struct LocalConfig {
    pub database_path: PathBuf,
    pub sqlite_db_path: PathBuf,
    pub blob_dir: PathBuf,
    pub capture_content: CapturePolicy,
}

#[async_trait::async_trait]
pub trait TraceStore: Send + Sync {
    async fn record_span(&self, span: SpanRecord) -> Result<()>;

    /// Record an intra-span event. Defaults to a no-op so existing stores keep
    /// compiling; stores that support events override this.
    async fn record_event(&self, _event: EventRecord) -> Result<()> {
        Ok(())
    }
}

pub struct DualRecorder {
    jsonl_file: Arc<Mutex<tokio::fs::File>>,
    events_file: Arc<Mutex<tokio::fs::File>>,
    sqlite: SqliteRecorder,
}

impl DualRecorder {
    pub async fn new(config: LocalConfig) -> Result<Self> {
        if let Some(parent) = config.database_path.parent() {
            fs::create_dir_all(parent).await?;
        }
        fs::create_dir_all(&config.blob_dir).await?;

        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&config.database_path)
            .await?;

        // Events live in a sibling JSONL file so the span stream stays
        // homogeneous (one SpanRecord per line).
        let events_path = config.database_path.with_extension("events.jsonl");
        let events_file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&events_path)
            .await?;

        let sqlite = SqliteRecorder::new(config.sqlite_db_path).await?;

        Ok(Self {
            jsonl_file: Arc::new(Mutex::new(file)),
            events_file: Arc::new(Mutex::new(events_file)),
            sqlite,
        })
    }
}

async fn append_jsonl<T: serde::Serialize>(file: &Mutex<tokio::fs::File>, value: &T) -> Result<()> {
    let json = serde_json::to_string(value)?;
    let mut file = file.lock().await;
    file.write_all(json.as_bytes()).await?;
    file.write_all(b"\n").await?;
    file.flush().await?;
    Ok(())
}

#[async_trait::async_trait]
impl TraceStore for DualRecorder {
    async fn record_span(&self, span: SpanRecord) -> Result<()> {
        append_jsonl(&self.jsonl_file, &span).await?;
        self.sqlite.record_span(span).await?;
        Ok(())
    }

    async fn record_event(&self, event: EventRecord) -> Result<()> {
        append_jsonl(&self.events_file, &event).await?;
        self.sqlite.record_event(event).await?;
        Ok(())
    }
}

pub type LocalRecorder = DualRecorder;
