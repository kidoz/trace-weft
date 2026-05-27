use anyhow::Result;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::fs::{self, OpenOptions};
use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex;
use trace_weft_core::{CapturePolicy, SpanRecord};

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
}

pub struct DualRecorder {
    jsonl_file: Arc<Mutex<tokio::fs::File>>,
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

        let sqlite = SqliteRecorder::new(config.sqlite_db_path).await?;

        Ok(Self {
            jsonl_file: Arc::new(Mutex::new(file)),
            sqlite,
        })
    }
}

#[async_trait::async_trait]
impl TraceStore for DualRecorder {
    async fn record_span(&self, span: SpanRecord) -> Result<()> {
        let json = serde_json::to_string(&span)?;
        let mut file = self.jsonl_file.lock().await;
        file.write_all(json.as_bytes()).await?;
        file.write_all(b"\n").await?;
        file.flush().await?;
        drop(file);

        self.sqlite.record_span(span).await?;
        Ok(())
    }
}

pub type LocalRecorder = DualRecorder;
