use crate::config::{AppConfig, DatabaseConfig, StorageConfig};
use crate::adapters::compression::gzip::GzipAdapter;
use crate::adapters::crypto::chacha::ChaChaAdapter;
use crate::adapters::database::postgres::PostgresAdapter;
use crate::adapters::database::mysql::MysqlAdapter;
use crate::adapters::database::sqlite::SqliteAdapter;
use crate::adapters::storage::s3::S3Adapter;
use crate::adapters::storage::gdrive::GDriveAdapter;
use crate::core::pipeline::BackupPipeline;
use anyhow::{anyhow, Result};
use std::sync::Arc;
use tokio_cron_scheduler::{Job, JobScheduler};

pub struct SchedulerService;

impl SchedulerService {
    pub async fn start(config: Arc<AppConfig>) -> Result<()> {
        let sched = JobScheduler::new().await?;

        for db in &config.databases {
            if let Some(cron_expr) = match db {
                DatabaseConfig::Postgres { cron, .. } => cron,
                DatabaseConfig::Mysql { cron, .. } => cron,
                DatabaseConfig::Sqlite { cron, .. } => cron,
            } {
                let db_clone = db.id().to_string();
                let config_clone = Arc::clone(&config);
                
                let job = Job::new_async(cron_expr.as_str(), move |_uuid, mut _l| {
                    let db_id = db_clone.clone();
                    let cfg = Arc::clone(&config_clone);
                    
                    Box::pin(async move {
                        tracing::info!("Cron triggered for DB: {}", db_id);
                        if let Err(e) = Self::run_backup_for_db(&db_id, &cfg).await {
                            tracing::error!("Cron backup failed for {}: {:?}", db_id, e);
                        }
                    })
                })?;

                sched.add(job).await?;
                tracing::info!("Scheduled cron {} for DB {}", cron_expr, db.id());
            }
        }

        tracing::info!("Scheduler successfully started. Waiting for jobs...");
        sched.start().await?;
        
        // Wait forever (the daemon blocks here)
        tokio::signal::ctrl_c().await?;
        tracing::info!("Shutting down daemon...");
        
        Ok(())
    }

    /// Helper to dynamically instantiate the adapters for a specific DB and run the pipeline
    pub async fn run_backup_for_db(db_id: &str, config: &AppConfig) -> Result<()> {
        // 1. Find DB Config
        let db_cfg = config.databases.iter().find(|d| d.id() == db_id)
            .ok_or_else(|| anyhow!("Database ID {} not found in config", db_id))?;
            
        // 2. Instantiate Encryption Adapter
        let encryptor: Box<dyn crate::ports::crypto::StreamEncryptor> = Box::new(ChaChaAdapter::new(config.security.encryption_key.as_bytes())?);
        
        // 3. Instantiate Compression Adapter
        let compressor: Box<dyn crate::ports::compression::StreamCompressor> = Box::new(GzipAdapter);
        
        // 4. Instantiate Storage Adapter
        let uploader = Self::build_storage_adapter(&config.storage).await?;

        // 5. Instantiate Database Adapter and run
        let start_time = std::time::Instant::now();
        let result = match db_cfg {
            DatabaseConfig::Postgres { connection_string, keep_last_n, .. } => {
                let extractor: Box<dyn crate::ports::database::DatabaseOperator> = Box::new(PostgresAdapter::new(connection_string));
                let pipeline = BackupPipeline::new(extractor, compressor, encryptor, uploader);
                pipeline.execute_backup(db_id, *keep_last_n).await
            }
            DatabaseConfig::Mysql { connection_string, keep_last_n, .. } => {
                let extractor: Box<dyn crate::ports::database::DatabaseOperator> = Box::new(MysqlAdapter::new(connection_string));
                let pipeline = BackupPipeline::new(extractor, compressor, encryptor, uploader);
                pipeline.execute_backup(db_id, *keep_last_n).await
            }
            DatabaseConfig::Sqlite { path, keep_last_n, .. } => {
                let extractor: Box<dyn crate::ports::database::DatabaseOperator> = Box::new(SqliteAdapter::new(path));
                let pipeline = BackupPipeline::new(extractor, compressor, encryptor, uploader);
                pipeline.execute_backup(db_id, *keep_last_n).await
            }
        };

        let time_taken_str = format!("{:.2?}", start_time.elapsed());

        match result {
            Ok(uploaded_size) => {
                tracing::info!(
                    "Backup successful for {}. Encrypted/compressed size: {} bytes. Time taken: {}",
                    db_id, uploaded_size, time_taken_str
                );
                
                if let Some(veevi) = &config.veevi {
                    let payload = crate::core::webhook::WebhookPayload {
                        database_id: db_id,
                        update_status: "success",
                        error_message: None,
                        storage_size: Some(uploaded_size),
                        time_taken: Some(&time_taken_str),
                    };
                    crate::core::webhook::notify_webhook(&veevi.license_key, &payload).await;
                }
                Ok(())
            }
            Err(e) => {
                tracing::error!("Backup failed for {}: {}", db_id, e);
                
                if let Some(veevi) = &config.veevi {
                    let error_msg = e.to_string();
                    let payload = crate::core::webhook::WebhookPayload {
                        database_id: db_id,
                        update_status: "failure",
                        error_message: Some(&error_msg),
                        storage_size: None,
                        time_taken: Some(&time_taken_str),
                    };
                    crate::core::webhook::notify_webhook(&veevi.license_key, &payload).await;
                }
                Err(e)
            }
        }
    }

    pub async fn run_restore_for_db(db_id: &str, backup_id: &str, config: &AppConfig, to_file: Option<String>) -> Result<()> {
        let db_cfg = config.databases.iter().find(|d| d.id() == db_id)
            .ok_or_else(|| anyhow!("Database ID {} not found in config", db_id))?;
            
        let encryptor: Box<dyn crate::ports::crypto::StreamEncryptor> = Box::new(ChaChaAdapter::new(config.security.encryption_key.as_bytes())?);
        let compressor: Box<dyn crate::ports::compression::StreamCompressor> = Box::new(GzipAdapter);
        
        let uploader = Self::build_storage_adapter(&config.storage).await?;

        match db_cfg {
            DatabaseConfig::Postgres { connection_string, .. } => {
                let extractor: Box<dyn crate::ports::database::DatabaseOperator> = Box::new(PostgresAdapter::new(connection_string));
                let pipeline = BackupPipeline::new(extractor, compressor, encryptor, uploader);
                pipeline.execute_restore(db_id, backup_id, to_file.clone()).await?;
            }
            DatabaseConfig::Mysql { connection_string, .. } => {
                let extractor: Box<dyn crate::ports::database::DatabaseOperator> = Box::new(MysqlAdapter::new(connection_string));
                let pipeline = BackupPipeline::new(extractor, compressor, encryptor, uploader);
                pipeline.execute_restore(db_id, backup_id, to_file.clone()).await?;
            }
            DatabaseConfig::Sqlite { path, .. } => {
                let extractor: Box<dyn crate::ports::database::DatabaseOperator> = Box::new(SqliteAdapter::new(path));
                let pipeline = BackupPipeline::new(extractor, compressor, encryptor, uploader);
                pipeline.execute_restore(db_id, backup_id, to_file).await?;
            }
        }
        Ok(())
    }

    pub async fn list_backups_for_db(db_id: &str, config: &AppConfig) -> Result<Vec<String>> {
        let uploader = Self::build_storage_adapter(&config.storage).await?;
        uploader.list_backups(db_id).await
    }

    pub async fn build_storage_adapter(storage_cfg: &StorageConfig) -> Result<Box<dyn crate::ports::storage::StorageOperator>> {
        let uploader: Box<dyn crate::ports::storage::StorageOperator> = match storage_cfg {
            StorageConfig::S3 { bucket, region, endpoint, access_key, secret_key } => {
                Box::new(S3Adapter::new(bucket, region, endpoint.as_deref(), access_key, secret_key))
            },
            StorageConfig::Gdrive { client_secret_file, token_cache_file } => {
                Box::new(GDriveAdapter::new(client_secret_file, token_cache_file).await?)
            },
            StorageConfig::R2 { bucket, account_id, access_key, secret_key } => {
                let endpoint = format!("https://{}.r2.cloudflarestorage.com", account_id);
                Box::new(S3Adapter::new(bucket, "auto", Some(&endpoint), access_key, secret_key))
            }
        };
        Ok(uploader)
    }
}
