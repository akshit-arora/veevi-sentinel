use async_trait::async_trait;
use tokio::io::AsyncRead;

#[async_trait]
pub trait DatabaseOperator: Send + Sync {
    /// Extracts the database and returns an AsyncRead stream of the dump
    async fn extract(&self) -> anyhow::Result<Box<dyn AsyncRead + Unpin + Send>>;
    
    /// Spawns the DB process and pipes the stream into its stdin
    async fn inject(&self, input: Box<dyn AsyncRead + Unpin + Send>) -> anyhow::Result<()>;
}
