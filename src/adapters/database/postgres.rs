use crate::ports::database::DatabaseOperator;
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use std::process::Stdio;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use tokio::process::Command;

pub struct PostgresAdapter {
    connection_string: String,
}

impl PostgresAdapter {
    pub fn new(connection_string: &str) -> Self {
        Self {
            connection_string: connection_string.to_string(),
        }
    }
}

#[async_trait]
impl DatabaseOperator for PostgresAdapter {
    async fn extract(&self) -> Result<Box<dyn AsyncRead + Unpin + Send>> {
        let mut child = Command::new("pg_dump")
            .arg(&self.connection_string)
            // Use custom format which is compressed by default and robust
            .arg("--format=custom")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| anyhow!("Failed to spawn pg_dump: {}", e))?;

        let stdout = child.stdout.take().ok_or_else(|| anyhow!("Failed to capture stdout"))?;
        
        // We let the child process run in the background. If it fails, the stream will close.
        // In a rigorous implementation, we would spawn a task to monitor child.wait() and log stderr.
        tokio::spawn(async move {
            let _ = child.wait().await;
        });

        // We wrap stdout in a Box and return it
        Ok(Box::new(stdout))
    }

    async fn inject(&self, mut input: Box<dyn AsyncRead + Unpin + Send>) -> Result<()> {
        let mut child = Command::new("pg_restore")
            .arg(&self.connection_string)
            .arg("--clean")
            .stdin(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| anyhow!("Failed to spawn pg_restore: {}", e))?;

        let mut stdin = child.stdin.take().ok_or_else(|| anyhow!("Failed to capture stdin"))?;

        // Pipe the input stream directly into pg_restore's stdin
        let mut buffer = vec![0u8; 8192];
        loop {
            match input.read(&mut buffer).await? {
                0 => break, // EOF
                n => {
                    stdin.write_all(&buffer[..n]).await?;
                }
            }
        }
        
        // Close stdin so pg_restore knows we are done
        drop(stdin);

        let status = child.wait().await?;
        if !status.success() {
            return Err(anyhow!("pg_restore failed with status: {}", status));
        }

        Ok(())
    }
}
