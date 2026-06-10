use std::collections::HashMap;

use sqlx::{Row, sqlite::SqlitePoolOptions};
use tempfile::TempDir;
use trace_weft_core::test_util::{sample_span_full, sample_span_minimal};
use trace_weft_core::{CapturePolicy, SpanRecord, TokenUsage};
use trace_weft_recorder::sqlite::SqliteRecorder;
use trace_weft_recorder::{DualRecorder, LocalConfig, TraceStore};

async fn open_pool(db_path: &std::path::Path) -> sqlx::SqlitePool {
    SqlitePoolOptions::new()
        .connect(&format!("sqlite://{}", db_path.to_string_lossy()))
        .await
        .expect("open recorded sqlite database")
}

#[tokio::test]
async fn sqlite_recorder_persists_full_span() {
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("traces.sqlite");
    let recorder = SqliteRecorder::new(db_path.clone()).await.unwrap();

    let span = sample_span_full();
    recorder.record_span(span.clone()).await.unwrap();

    let pool = open_pool(&db_path).await;
    let row = sqlx::query("SELECT * FROM spans WHERE span_id = ?")
        .bind(span.span_id.0.to_string())
        .fetch_one(&pool)
        .await
        .unwrap();

    assert_eq!(
        row.get::<String, _>("trace_id"),
        span.trace_id.0.to_string()
    );
    assert_eq!(row.get::<String, _>("run_id"), span.run_id.0.to_string());
    assert_eq!(
        row.get::<Option<String>, _>("parent_span_id"),
        span.parent_span_id.map(|id| id.0.to_string())
    );
    assert_eq!(row.get::<String, _>("span_kind"), "llm_call");
    assert_eq!(row.get::<String, _>("status"), "ok");
    assert_eq!(row.get::<String, _>("redaction_policy"), "redacted_preview");
    assert_eq!(row.get::<String, _>("name"), "draft_answer");
    assert_eq!(row.get::<i64, _>("start_time") as u64, span.start_time);
    assert_eq!(
        row.get::<Option<i64>, _>("end_time").map(|t| t as u64),
        span.end_time
    );
    assert_eq!(
        row.get::<Option<String>, _>("model_provider").as_deref(),
        Some("openai")
    );
    assert_eq!(
        row.get::<Option<String>, _>("model_name").as_deref(),
        Some("gpt-4.1")
    );
    assert_eq!(row.get::<Option<bool>, _>("cache_hit"), Some(false));
    assert_eq!(row.get::<Option<i64>, _>("retry_count"), Some(1));

    // JSON columns must parse back to the original values.
    let attributes: HashMap<String, serde_json::Value> =
        serde_json::from_str(&row.get::<String, _>("attributes")).unwrap();
    assert_eq!(attributes, span.attributes);

    let token_usage: TokenUsage =
        serde_json::from_str(&row.get::<Option<String>, _>("token_usage").unwrap()).unwrap();
    assert_eq!(Some(token_usage), span.token_usage);

    let input_ref: trace_weft_core::BlobRef =
        serde_json::from_str(&row.get::<Option<String>, _>("input_ref").unwrap()).unwrap();
    assert_eq!(Some(input_ref), span.input_ref);
}

#[tokio::test]
async fn sqlite_recorder_persists_minimal_span_with_nulls() {
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("traces.sqlite");
    let recorder = SqliteRecorder::new(db_path.clone()).await.unwrap();

    let span = sample_span_minimal();
    recorder.record_span(span.clone()).await.unwrap();

    let pool = open_pool(&db_path).await;
    let row = sqlx::query("SELECT * FROM spans WHERE span_id = ?")
        .bind(span.span_id.0.to_string())
        .fetch_one(&pool)
        .await
        .unwrap();

    assert_eq!(row.get::<String, _>("span_kind"), "tool");
    assert_eq!(row.get::<String, _>("status"), "in_progress");
    assert_eq!(row.get::<String, _>("redaction_policy"), "metadata_only");
    assert_eq!(row.get::<Option<String>, _>("parent_span_id"), None);
    assert_eq!(row.get::<Option<i64>, _>("end_time"), None);
    assert_eq!(row.get::<Option<String>, _>("token_usage"), None);
    assert_eq!(row.get::<Option<String>, _>("input_ref"), None);
    assert_eq!(row.get::<Option<bool>, _>("cache_hit"), None);
}

#[tokio::test]
async fn dual_recorder_writes_jsonl_and_sqlite() {
    let dir = TempDir::new().unwrap();
    // Nested paths verify that missing parent directories are created.
    let config = LocalConfig {
        database_path: dir.path().join("nested/traces.jsonl"),
        sqlite_db_path: dir.path().join("nested/traces.sqlite"),
        blob_dir: dir.path().join("nested/blobs"),
        capture_content: CapturePolicy::RedactedPreview,
    };
    let recorder = DualRecorder::new(config.clone()).await.unwrap();

    let spans = vec![sample_span_full(), sample_span_minimal()];
    for span in &spans {
        recorder.record_span(span.clone()).await.unwrap();
    }

    assert!(config.blob_dir.is_dir(), "blob directory should be created");

    // JSONL: one line per span, each parsing back to the original record.
    let jsonl = std::fs::read_to_string(&config.database_path).unwrap();
    let recorded: Vec<SpanRecord> = jsonl
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(recorded, spans);

    // SQLite: both spans stored.
    let pool = open_pool(&config.sqlite_db_path).await;
    let count: i64 = sqlx::query("SELECT COUNT(*) AS n FROM spans")
        .fetch_one(&pool)
        .await
        .unwrap()
        .get("n");
    assert_eq!(count, 2);
}
