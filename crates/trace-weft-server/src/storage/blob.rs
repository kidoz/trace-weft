use bytes::Bytes;
use object_store::{ObjectStore, path::Path as ObjectPath};
use std::path::PathBuf;
use tokio::fs;
use trace_weft_core::{BlobHash, BlobStore};

pub struct LocalBlobStore {
    dir: PathBuf,
}

impl LocalBlobStore {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }
}

#[async_trait::async_trait]
impl BlobStore for LocalBlobStore {
    async fn put_blob(
        &self,
        hash: &BlobHash,
        _content_type: &str,
        content: &[u8],
    ) -> anyhow::Result<()> {
        let path = self.dir.join(&hash.0);
        fs::write(path, content).await?;
        Ok(())
    }

    async fn get_blob(&self, hash: &BlobHash) -> anyhow::Result<Option<Vec<u8>>> {
        let path = self.dir.join(&hash.0);
        if path.exists() {
            let data = fs::read(path).await?;
            Ok(Some(data))
        } else {
            Ok(None)
        }
    }
}

pub struct S3BlobStore {
    store: Box<dyn ObjectStore>,
}

impl S3BlobStore {
    pub fn new(
        bucket: &str,
        region: &str,
        access_key: &str,
        secret_key: &str,
    ) -> anyhow::Result<Self> {
        let store = object_store::aws::AmazonS3Builder::new()
            .with_bucket_name(bucket)
            .with_region(region)
            .with_access_key_id(access_key)
            .with_secret_access_key(secret_key)
            .build()?;

        Ok(Self {
            store: Box::new(store),
        })
    }
}

#[async_trait::async_trait]
impl BlobStore for S3BlobStore {
    async fn put_blob(
        &self,
        hash: &BlobHash,
        _content_type: &str,
        content: &[u8],
    ) -> anyhow::Result<()> {
        let path = ObjectPath::from(hash.0.clone());
        let bytes = Bytes::copy_from_slice(content);
        self.store.put(&path, bytes.into()).await?;
        Ok(())
    }

    async fn get_blob(&self, hash: &BlobHash) -> anyhow::Result<Option<Vec<u8>>> {
        let path = ObjectPath::from(hash.0.clone());
        match self.store.get(&path).await {
            Ok(result) => {
                let bytes = result.bytes().await?;
                Ok(Some(bytes.to_vec()))
            }
            Err(object_store::Error::NotFound { .. }) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }
}
