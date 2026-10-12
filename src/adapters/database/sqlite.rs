use crate::ports::database::DatabaseOperator;
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use std::process::Stdio;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use tokio::process::Command;

pub struct SqliteAdapter {
    path: String,
}

impl SqliteAdapter {
    pub fn new(path: &str) -> Self {
        Self {
            path: path.to_string(),
        }
    }
}

#[async_trait]
impl DatabaseOperator for SqliteAdapter {
    async fn extract(&self) -> Result<Box<dyn AsyncRead + Unpin + Send>> {
        // Run: sqlite3 <path> .dump
        let mut child = Command::new("sqlite3")
            .arg(&self.path)
            .arg(".dump")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| anyhow!("Failed to spawn sqlite3: {}", e))?;

        let stdout = child.stdout.take().ok_or_else(|| anyhow!("Failed to capture stdout"))?;
        
        tokio::spawn(async move {
            let _ = child.wait().await;
        });

        Ok(Box::new(stdout))
    }

    async fn inject(&self, mut input: Box<dyn AsyncRead + Unpin + Send>) -> Result<()> {
        // Run: sqlite3 <path> 
        // We will pipe the SQL statements into sqlite3 to reconstruct the DB
        let mut child = Command::new("sqlite3")
            .arg(&self.path)
            .stdin(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| anyhow!("Failed to spawn sqlite3 for injection: {}", e))?;

        let mut stdin = child.stdin.take().ok_or_else(|| anyhow!("Failed to capture stdin"))?;

        let mut buffer = vec![0u8; 8192];
        loop {
            match input.read(&mut buffer).await? {
                0 => break, 
                n => {
                    stdin.write_all(&buffer[..n]).await?;
                }
            }
        }
        
        drop(stdin);

        let status = child.wait().await?;
        if !status.success() {
            return Err(anyhow!("sqlite3 restore failed with status: {}", status));
        }

        Ok(())
    }
}
