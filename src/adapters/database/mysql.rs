use crate::ports::database::DatabaseOperator;
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use std::process::Stdio;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use tokio::process::Command;
use url::Url;

pub struct MysqlAdapter {
    connection_string: String,
}

impl MysqlAdapter {
    pub fn new(connection_string: &str) -> Self {
        Self {
            connection_string: connection_string.to_string(),
        }
    }

    /// Parses the mysql:// URI to extract arguments for mysqldump/mysql CLI
    fn parse_uri(&self) -> Result<Vec<String>> {
        let parsed_url = Url::parse(&self.connection_string)
            .map_err(|e| anyhow!("Failed to parse MySQL connection string as URL: {}", e))?;

        let scheme = parsed_url.scheme();
        if scheme != "mysql" && scheme != "mariadb" {
            return Err(anyhow!("MySQL/MariaDB connection string must start with mysql:// or mariadb://"));
        }

        let mut args = Vec::new();

        if let Some(host) = parsed_url.host_str() {
            args.push(format!("--host={}", host));
        }

        if let Some(port) = parsed_url.port() {
            args.push(format!("--port={}", port));
        }

        let username = parsed_url.username();
        if !username.is_empty() {
            args.push(format!("--user={}", username));
        }

        if let Some(password) = parsed_url.password() {
            args.push(format!("--password={}", password));
        }

        // The database name is the path without the leading slash
        let db_name = parsed_url.path().trim_start_matches('/');
        if !db_name.is_empty() {
            args.push(db_name.to_string());
        }

        Ok(args)
    }
}

#[async_trait]
impl DatabaseOperator for MysqlAdapter {
    async fn extract(&self) -> Result<Box<dyn AsyncRead + Unpin + Send>> {
        let args = self.parse_uri()?;
        
        let mut child = Command::new("mysqldump")
            .args(&args)
            .arg("--single-transaction")
            .arg("--quick")
            .arg("--routines")
            .arg("--triggers")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| anyhow!("Failed to spawn mysqldump: {}", e))?;

        let stdout = child.stdout.take().ok_or_else(|| anyhow!("Failed to capture stdout"))?;
        
        tokio::spawn(async move {
            let _ = child.wait().await;
        });

        Ok(Box::new(stdout))
    }

    async fn inject(&self, mut input: Box<dyn AsyncRead + Unpin + Send>) -> Result<()> {
        let args = self.parse_uri()?;
        
        let mut child = Command::new("mysql")
            .args(&args)
            .stdin(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| anyhow!("Failed to spawn mysql: {}", e))?;

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
            return Err(anyhow!("mysql restore failed with status: {}", status));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_uri_mysql() {
        let adapter = MysqlAdapter::new("mysql://user:pass@127.0.0.1:3306/mydb");
        let args = adapter.parse_uri().unwrap();
        assert!(args.contains(&"--host=127.0.0.1".to_string()));
        assert!(args.contains(&"--port=3306".to_string()));
        assert!(args.contains(&"--user=user".to_string()));
        assert!(args.contains(&"--password=pass".to_string()));
        assert!(args.contains(&"mydb".to_string()));
    }

    #[test]
    fn test_parse_uri_mariadb() {
        let adapter = MysqlAdapter::new("mariadb://user@localhost/db2");
        let args = adapter.parse_uri().unwrap();
        assert!(args.contains(&"--host=localhost".to_string()));
        assert!(args.contains(&"--user=user".to_string()));
        assert!(!args.iter().any(|a| a.starts_with("--password=")));
        assert!(args.contains(&"db2".to_string()));
    }

    #[test]
    fn test_parse_uri_invalid_scheme() {
        let adapter = MysqlAdapter::new("postgres://user:pass@localhost/db");
        assert!(adapter.parse_uri().is_err());
    }
}
