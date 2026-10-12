use serde::{Deserialize, Serialize};
use std::fs;
use anyhow::{Context, Result};

#[derive(Debug, Deserialize, Serialize)]
pub struct AppConfig {
    pub databases: Vec<DatabaseConfig>,
    pub security: SecurityConfig,
    pub storage: StorageConfig,
    pub logging: LoggingConfig,
    pub veevi: Option<VeeviConfig>,
}

impl AppConfig {
    /// Discovers configuration file: checks custom path, or automatically falls back to config.yaml and config.yml.
    pub fn find_config_path(custom: Option<&str>) -> Option<String> {
        if let Some(path) = custom {
            return Some(path.to_string());
        }
        for candidate in ["config.yaml", "config.yml"] {
            if std::path::Path::new(candidate).exists() {
                return Some(candidate.to_string());
            }
        }
        None
    }

    pub fn load(path: &str) -> Result<Self> {
        let content = fs::read_to_string(path)
            .with_context(|| format!("Failed to read configuration file at '{}'", path))?;
        let config: AppConfig = serde_yaml::from_str(&content)
            .with_context(|| "Failed to parse YAML configuration")?;
        Ok(config)
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum DatabaseConfig {
    Postgres {
        id: String,
        connection_string: String,
        cron: Option<String>,
        keep_last_n: Option<usize>,
    },
    Mysql {
        id: String,
        connection_string: String,
        cron: Option<String>,
        keep_last_n: Option<usize>,
    },
    Sqlite {
        id: String,
        path: String,
        cron: Option<String>,
        keep_last_n: Option<usize>,
    },
}

impl DatabaseConfig {
    pub fn id(&self) -> &str {
        match self {
            Self::Postgres { id, .. } => id,
            Self::Mysql { id, .. } => id,
            Self::Sqlite { id, .. } => id,
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct SecurityConfig {
    pub encryption_algo: String,
    pub encryption_key: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum StorageConfig {
    S3 {
        bucket: String,
        region: String,
        endpoint: Option<String>,
        access_key: String,
        secret_key: String,
    },
    Gdrive {
        client_secret_file: String,
        token_cache_file: String,
    },
    R2 {
        bucket: String,
        account_id: String,
        access_key: String,
        secret_key: String,
    },
    Local {
        path: String,
    },
}

#[derive(Debug, Deserialize, Serialize)]
pub struct LoggingConfig {
    pub level: String,
    #[serde(rename = "type")]
    pub log_type: String,
    pub file_path: String,
    pub file_prefix: String,
    pub purge_after_days: Option<usize>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct VeeviConfig {
    pub license_key: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_custom_config_path() {
        assert_eq!(AppConfig::find_config_path(Some("custom.yaml")), Some("custom.yaml".to_string()));
    }
}
