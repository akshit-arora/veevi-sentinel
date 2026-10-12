use async_trait::async_trait;
use tokio::io::AsyncRead;

#[async_trait]
pub trait StorageOperator: Send + Sync {
    /// Uploads a stream to the storage bucket
    async fn upload(&self, key: &str, input: Box<dyn AsyncRead + Unpin + Send>) -> anyhow::Result<()>;
    
    /// Downloads a stream from the storage bucket
    async fn download(&self, key: &str) -> anyhow::Result<Box<dyn AsyncRead + Unpin + Send>>;
    
    /// Lists all backups for a given db_id
    async fn list_backups(&self, db_id: &str) -> anyhow::Result<Vec<String>>;
    
    /// Deletes older backups beyond the keep_last_n limit
    async fn prune_backups(&self, db_id: &str, keep_last_n: usize) -> anyhow::Result<()>;
}
