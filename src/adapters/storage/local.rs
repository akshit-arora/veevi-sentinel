use crate::ports::storage::StorageOperator;
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use std::path::PathBuf;
use tokio::io::AsyncRead;

pub struct LocalAdapter {
    base_path: PathBuf,
}

impl LocalAdapter {
    pub fn new(base_path: impl Into<PathBuf>) -> Self {
        Self {
            base_path: base_path.into(),
        }
    }
}

#[async_trait]
impl StorageOperator for LocalAdapter {
    async fn upload(&self, key: &str, mut input: Box<dyn AsyncRead + Unpin + Send>) -> Result<()> {
        let file_path = self.base_path.join(key);
        if let Some(parent) = file_path.parent() {
            tokio::fs::create_dir_all(parent).await
                .map_err(|e| anyhow!("Failed to create local storage directory {:?}: {}", parent, e))?;
        }

        let mut file = tokio::fs::File::create(&file_path).await
            .map_err(|e| anyhow!("Failed to create local destination file {:?}: {}", file_path, e))?;

        tokio::io::copy(&mut input, &mut file).await
            .map_err(|e| anyhow!("Failed to write to local storage file {:?}: {}", file_path, e))?;

        Ok(())
    }

    async fn download(&self, key: &str) -> Result<Box<dyn AsyncRead + Unpin + Send>> {
        let file_path = self.base_path.join(key);
        let file = tokio::fs::File::open(&file_path).await
            .map_err(|e| anyhow!("Failed to open local storage file {:?}: {}", file_path, e))?;
        Ok(Box::new(file))
    }

    async fn list_backups(&self, db_id: &str) -> Result<Vec<String>> {
        let db_dir = self.base_path.join(db_id);
        if !db_dir.exists() {
            return Ok(Vec::new());
        }

        let mut keys = Vec::new();
        let mut entries = tokio::fs::read_dir(&db_dir).await
            .map_err(|e| anyhow!("Failed to read local storage directory {:?}: {}", db_dir, e))?;

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.is_file() {
                if let Some(file_name) = path.file_name().and_then(|n| n.to_str()) {
                    keys.push(format!("{}/{}", db_id, file_name));
                }
            }
        }

        keys.sort();
        Ok(keys)
    }

    async fn prune_backups(&self, db_id: &str, keep_last_n: usize) -> Result<()> {
        let keys = self.list_backups(db_id).await?;
        if keys.len() <= keep_last_n {
            return Ok(());
        }

        let to_delete = keys.len() - keep_last_n;
        for key in keys.iter().take(to_delete) {
            let file_path = self.base_path.join(key);
            if file_path.exists() {
                tokio::fs::remove_file(&file_path).await
                    .map_err(|e| anyhow!("Failed to prune local backup {:?}: {}", file_path, e))?;
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    use tokio::io::AsyncReadExt;

    #[tokio::test]
    async fn test_local_storage_lifecycle() -> Result<()> {
        let dir = tempdir()?;
        let adapter = LocalAdapter::new(dir.path());

        // Test upload
        let data = b"hello encrypted backup stream";
        let input: Box<dyn AsyncRead + Unpin + Send> = Box::new(&data[..]);
        adapter.upload("test-db/20261012_000000.sql.gz.enc", input).await?;

        // Test list
        let backups = adapter.list_backups("test-db").await?;
        assert_eq!(backups.len(), 1);
        assert_eq!(backups[0], "test-db/20261012_000000.sql.gz.enc");

        // Test download
        let mut reader = adapter.download("test-db/20261012_000000.sql.gz.enc").await?;
        let mut downloaded = Vec::new();
        reader.read_to_end(&mut downloaded).await?;
        assert_eq!(downloaded, data);

        // Test prune
        let input2: Box<dyn AsyncRead + Unpin + Send> = Box::new(&data[..]);
        adapter.upload("test-db/20261012_000001.sql.gz.enc", input2).await?;
        assert_eq!(adapter.list_backups("test-db").await?.len(), 2);

        adapter.prune_backups("test-db", 1).await?;
        let remaining = adapter.list_backups("test-db").await?;
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0], "test-db/20261012_000001.sql.gz.enc");

        Ok(())
    }
}
