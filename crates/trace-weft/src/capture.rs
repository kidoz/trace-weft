//! Content capture.
//!
//! Turns serialized inputs/outputs into [`BlobRef`]s and persists the bytes to
//! a [`BlobStore`], gated by the configured [`CapturePolicy`]. The
//! instrumentation macros and `SpanBuilder` call [`capture_json`] to populate a
//! span's `input_ref` / `output_ref`.
//!
//! Capture is opt-in at the process level: until [`init_capture`] runs (which
//! `init_local` does for you), [`capture_enabled`] is `false` and nothing is
//! serialized or stored, so a `MetadataOnly` deployment pays no cost.

use std::sync::Arc;
use tokio::sync::OnceCell;
use trace_weft_core::{
    BlobHash, BlobRef, BlobStore, CapturePolicy, RedactionStatus, redactor::ArcRedactor,
};

const PREVIEW_MAX_BYTES: usize = 512;

/// Process-wide capture configuration.
pub struct CaptureConfig {
    pub policy: CapturePolicy,
    pub blobs: Arc<dyn BlobStore>,
    pub redactor: ArcRedactor,
    /// Recorded verbatim into `BlobRef::storage_backend` (e.g. `"local_fs"`).
    pub storage_backend: String,
}

static CAPTURE: OnceCell<CaptureConfig> = OnceCell::const_new();

/// Install the process-wide capture configuration. Errors if already set.
pub fn init_capture(config: CaptureConfig) -> anyhow::Result<()> {
    CAPTURE
        .set(config)
        .map_err(|_| anyhow::anyhow!("Capture already initialized"))?;
    Ok(())
}

/// Whether content capture is active. `false` when capture is uninitialized or
/// the policy is `MetadataOnly`; callers should skip serialization entirely in
/// that case.
pub fn capture_enabled() -> bool {
    CAPTURE
        .get()
        .is_some_and(|c| !matches!(c.policy, CapturePolicy::MetadataOnly))
}

/// Serialize already-built JSON content into a stored blob and return a
/// [`BlobRef`] describing it, honoring the configured policy. Returns `None`
/// when capture is disabled.
pub async fn capture_json(content_type: &str, value: serde_json::Value) -> Option<BlobRef> {
    let cfg = CAPTURE.get()?;
    if matches!(cfg.policy, CapturePolicy::MetadataOnly) {
        return None;
    }

    let raw = serde_json::to_vec(&value).ok()?;
    let raw_text = String::from_utf8_lossy(&raw);

    let (stored_bytes, redaction_status, preview_text) = match cfg.policy {
        CapturePolicy::RedactedPreview => {
            let result = cfg.redactor.redact(&raw_text);
            let preview = preview(&result.redacted_text);
            (result.redacted_text.into_bytes(), result.status, preview)
        }
        CapturePolicy::FullContentLocalOnly | CapturePolicy::FullContentExportable => {
            let preview = preview(&raw_text);
            (raw.clone(), RedactionStatus::Unredacted, preview)
        }
        CapturePolicy::MetadataOnly => return None,
    };

    let hash = BlobHash(sha256_hex(&stored_bytes));
    let size_bytes = stored_bytes.len() as u64;

    // A failed blob write must not lose the span; record the ref regardless.
    if let Err(err) = cfg.blobs.put_blob(&hash, content_type, &stored_bytes).await {
        tracing::warn!(error = %err, "failed to persist captured blob");
    }

    Some(BlobRef {
        hash,
        content_type: content_type.to_string(),
        size_bytes,
        created_at_timestamp: now_ms(),
        redaction_status,
        encryption_status: "none".to_string(),
        storage_backend: cfg.storage_backend.clone(),
        preview_text_redacted: Some(preview_text),
    })
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("sha256:{:x}", hasher.finalize())
}

fn preview(text: &str) -> String {
    if text.len() <= PREVIEW_MAX_BYTES {
        return text.to_string();
    }
    let mut end = PREVIEW_MAX_BYTES;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &text[..end])
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// Filesystem blob store: writes each blob to `dir/<hash>`. Used by
/// `init_local`.
pub struct FsBlobStore {
    dir: std::path::PathBuf,
}

impl FsBlobStore {
    pub fn new(dir: impl Into<std::path::PathBuf>) -> Self {
        Self { dir: dir.into() }
    }
}

#[async_trait::async_trait]
impl BlobStore for FsBlobStore {
    async fn put_blob(
        &self,
        hash: &BlobHash,
        _content_type: &str,
        content: &[u8],
    ) -> anyhow::Result<()> {
        // Hashes are prefixed (`sha256:`); ':' is not portable in filenames.
        let path = self.dir.join(hash.0.replace(':', "_"));
        tokio::fs::write(path, content).await?;
        Ok(())
    }

    async fn get_blob(&self, hash: &BlobHash) -> anyhow::Result<Option<Vec<u8>>> {
        let path = self.dir.join(hash.0.replace(':', "_"));
        match tokio::fs::read(path).await {
            Ok(bytes) => Ok(Some(bytes)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.into()),
        }
    }
}

/// In-memory blob store, handy for tests and ephemeral runs.
#[derive(Clone, Default)]
pub struct MemoryBlobStore {
    blobs: Arc<std::sync::Mutex<std::collections::HashMap<String, Vec<u8>>>>,
}

impl MemoryBlobStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.blobs.lock().unwrap().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[async_trait::async_trait]
impl BlobStore for MemoryBlobStore {
    async fn put_blob(
        &self,
        hash: &BlobHash,
        _content_type: &str,
        content: &[u8],
    ) -> anyhow::Result<()> {
        self.blobs
            .lock()
            .unwrap()
            .insert(hash.0.clone(), content.to_vec());
        Ok(())
    }

    async fn get_blob(&self, hash: &BlobHash) -> anyhow::Result<Option<Vec<u8>>> {
        Ok(self.blobs.lock().unwrap().get(&hash.0).cloned())
    }
}
