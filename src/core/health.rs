use crate::config::{AppConfig, DatabaseConfig};
use anyhow::{anyhow, Result};
use std::process::Stdio;
use tokio::process::Command;
use std::sync::Arc;

pub struct HealthCheckService;

impl HealthCheckService {
    /// Aggressively validates the configuration on startup.
    /// Fails fast if connection strings are malformed or credentials are bad.
    pub fn validate(config: &AppConfig) -> Result<()> {
        // 1. Validate Encryption Algorithm & Key
        let algo = config.security.encryption_algo.to_lowercase();
        let algo_clean = algo.replace(['-', '_'], "");
        if algo_clean != "xchacha20poly1305" && algo_clean != "chacha20poly1305" {
            return Err(anyhow!(
                "Unsupported encryption algorithm '{}'. Supported algorithms: 'xchacha20poly1305' (or 'chacha20poly1305')",
                config.security.encryption_algo
            ));
        }

        let key_bytes = config.security.encryption_key.as_bytes();
        if key_bytes.len() != 32 {
            return Err(anyhow!(
                "Encryption key must be exactly 32 bytes for XChaCha20-Poly1305. Current length is {}",
                key_bytes.len()
            ));
        }

        // 2. Validate Database Configurations
        for db in &config.databases {
            match db {
                crate::config::DatabaseConfig::Postgres { connection_string, .. } => {
                    if !connection_string.starts_with("postgres://") && !connection_string.starts_with("postgresql://") {
                        return Err(anyhow!("Invalid PostgreSQL connection string for DB ID: {}", db.id()));
                    }
                }
                crate::config::DatabaseConfig::Mysql { connection_string, .. } => {
                    if !connection_string.starts_with("mysql://") && !connection_string.starts_with("mariadb://") {
                        return Err(anyhow!("Invalid MySQL/MariaDB connection string for DB ID: {}", db.id()));
                    }
                }
                crate::config::DatabaseConfig::Sqlite { path, .. } => {
                    if path.trim().is_empty() {
                        return Err(anyhow!("SQLite path cannot be empty for DB ID: {}", db.id()));
                    }
                }
            }
        }

        // 3. Validate Storage Settings
        match &config.storage {
            crate::config::StorageConfig::S3 { access_key, secret_key, .. } => {
                if access_key.trim().is_empty() || secret_key.trim().is_empty() {
                    return Err(anyhow!("S3 access_key and secret_key cannot be empty"));
                }
            }
            crate::config::StorageConfig::Gdrive { client_secret_file, .. } => {
                if !std::path::Path::new(client_secret_file).exists() {
                    return Err(anyhow!("Google Drive client secret file not found at: {}", client_secret_file));
                }
            }
            crate::config::StorageConfig::R2 { access_key, secret_key, account_id, .. } => {
                if access_key.trim().is_empty() || secret_key.trim().is_empty() || account_id.trim().is_empty() {
                    return Err(anyhow!("R2 access_key, secret_key, and account_id cannot be empty"));
                }
            }
        }

        // 4. Validate Veevi Config
        if let Some(veevi) = &config.veevi {
            if veevi.license_key.trim().is_empty() {
                return Err(anyhow!("Veevi license_key cannot be empty"));
            }
        }

        Ok(())
    }

    /// Performs a live connectivity check on storage and verifies local DB binaries.
    pub async fn live_check(config: Arc<AppConfig>, target: Option<String>) -> Result<()> {
        println!("🚀 Starting live dry-run health check...");

        // 1. Verify Storage Connectivity
        println!("☁️  Testing cloud storage connectivity...");
        let storage_operator = crate::core::scheduler::SchedulerService::build_storage_adapter(&config.storage).await?;
        match storage_operator.list_backups("dry-run-test-id").await {
            Ok(_) => println!("✅ Cloud storage authenticated successfully!"),
            Err(e) => return Err(anyhow!("Storage authentication failed: {}", e)),
        }

        // 2. Verify Database Utilities
        println!("💾 Testing database utilities...");
        let dbs_to_check: Vec<&DatabaseConfig> = match &target {
            Some(id) => {
                let db = config.databases.iter().find(|d| d.id() == id)
                    .ok_or_else(|| anyhow!("Database ID '{}' not found in config", id))?;
                vec![db]
            }
            None => config.databases.iter().collect(),
        };

        for db in dbs_to_check {
            let (binary, args) = match db {
                DatabaseConfig::Postgres { .. } => ("pg_dump", vec!["--version"]),
                DatabaseConfig::Mysql { .. } => ("mysqldump", vec!["--version"]),
                DatabaseConfig::Sqlite { .. } => ("sqlite3", vec!["--version"]),
            };

            print!("   Checking for {} (used by {})... ", binary, db.id());
            let status = Command::new(binary)
                .args(&args)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .await;

            match status {
                Ok(s) if s.success() => println!("✅ OK"),
                _ => return Err(anyhow!("Failed to execute '{}'. Is it installed and in your PATH?", binary)),
            }
        }

        println!("🎉 Dry-run complete! All systems go.");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{AppConfig, SecurityConfig, StorageConfig, LoggingConfig, DatabaseConfig};

    fn get_valid_config() -> AppConfig {
        AppConfig {
            databases: vec![
                DatabaseConfig::Postgres {
                    id: "pg".to_string(),
                    connection_string: "postgres://user:pass@localhost/db".to_string(),
                    cron: None,
                    keep_last_n: None,
                }
            ],
            security: SecurityConfig {
                encryption_algo: "xchacha20poly1305".to_string(),
                encryption_key: "12345678901234567890123456789012".to_string(),
            },
            storage: StorageConfig::S3 {
                bucket: "b".to_string(),
                region: "r".to_string(),
                endpoint: None,
                access_key: "a".to_string(),
                secret_key: "s".to_string(),
            },
            logging: LoggingConfig {
                level: "info".to_string(),
                log_type: "single".to_string(),
                file_path: "/tmp".to_string(),
                file_prefix: "log".to_string(),
                purge_after_days: None,
            },
            veevi: None,
        }
    }

    #[test]
    fn test_valid_config() {
        let config = get_valid_config();
        assert!(HealthCheckService::validate(&config).is_ok());
    }

    #[test]
    fn test_legacy_algo_alias() {
        let mut config = get_valid_config();
        config.security.encryption_algo = "chacha20poly1305".to_string();
        assert!(HealthCheckService::validate(&config).is_ok());

        config.security.encryption_algo = "XChaCha20-Poly1305".to_string();
        assert!(HealthCheckService::validate(&config).is_ok());
    }

    #[test]
    fn test_invalid_algo() {
        let mut config = get_valid_config();
        config.security.encryption_algo = "aes256".to_string();
        assert!(HealthCheckService::validate(&config).is_err());
    }

    #[test]
    fn test_invalid_key_length() {
        let mut config = get_valid_config();
        config.security.encryption_key = "short".to_string();
        assert!(HealthCheckService::validate(&config).is_err());
    }

    #[test]
    fn test_invalid_db_uri() {
        let mut config = get_valid_config();
        config.databases = vec![
            DatabaseConfig::Mysql {
                id: "my".to_string(),
                connection_string: "postgres://user:pass@localhost/db".to_string(),
                cron: None,
                keep_last_n: None,
            }
        ];
        assert!(HealthCheckService::validate(&config).is_err());
    }
}
