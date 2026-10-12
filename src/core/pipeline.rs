use crate::ports::{
    compression::StreamCompressor, crypto::StreamEncryptor, database::DatabaseOperator,
    storage::StorageOperator,
};
use anyhow::{anyhow, Result};
use chrono::Utc;

use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, ReadBuf};

pub struct ByteCounter<R> {
    inner: R,
    pub count: Arc<AtomicU64>,
}

impl<R: AsyncRead + Unpin> ByteCounter<R> {
    pub fn new(inner: R, count: Arc<AtomicU64>) -> Self {
        Self { inner, count }
    }
}

impl<R: AsyncRead + Unpin> AsyncRead for ByteCounter<R> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        let before = buf.filled().len();
        let poll = Pin::new(&mut self.inner).poll_read(cx, buf);
        if let Poll::Ready(Ok(())) = &poll {
            let after = buf.filled().len();
            self.count.fetch_add((after - before) as u64, Ordering::Relaxed);
        }
        poll
    }
}

pub struct BackupPipeline {
    extractor: Box<dyn DatabaseOperator>,
    compressor: Box<dyn StreamCompressor>,
    encryptor: Box<dyn StreamEncryptor>,
    uploader: Box<dyn StorageOperator>,
}

impl BackupPipeline {
    pub fn new(
        extractor: Box<dyn DatabaseOperator>, 
        compressor: Box<dyn StreamCompressor>, 
        encryptor: Box<dyn StreamEncryptor>, 
        uploader: Box<dyn StorageOperator>
    ) -> Self {
        Self {
            extractor,
            compressor,
            encryptor,
            uploader,
        }
    }

    /// Executes the forward backup pipeline: Extract -> Compress -> Encrypt -> Upload
    pub async fn execute_backup(&self, db_id: &str, keep_last_n: Option<usize>) -> Result<u64> {
        tracing::info!("Starting backup pipeline for database: {}", db_id);

        let dump_stream = self.extractor.extract().await?;
        let compressed_stream = self.compressor.compress_stream(dump_stream);
        let encrypted_stream = self.encryptor.encrypt_stream(compressed_stream);

        let count = Arc::new(AtomicU64::new(0));
        let counter_stream = ByteCounter::new(encrypted_stream, Arc::clone(&count));

        // Generate a unique filename using timestamp
        let timestamp = Utc::now().format("%Y%m%d_%H%M%S");
        let key = format!("{}/{}.sql.gz.enc", db_id, timestamp);

        self.uploader.upload(&key, Box::new(counter_stream)).await?;
        
        let uploaded_size = count.load(Ordering::Relaxed);
        tracing::info!("Backup successfully uploaded to: {} (Size: {} bytes)", key, uploaded_size);

        if let Some(keep) = keep_last_n {
            tracing::info!("Applying retention policy (keep_last_n = {}) for {}", keep, db_id);
            if let Err(e) = self.uploader.prune_backups(db_id, keep).await {
                tracing::error!("Failed to prune old backups for {}: {:?}", db_id, e);
            }
        }

        Ok(uploaded_size)
    }

    /// Executes the reverse restore pipeline: Download -> Decrypt -> Decompress -> Inject/File
    pub async fn execute_restore(&self, db_id: &str, backup_id: &str, to_file: Option<String>) -> Result<()> {
        tracing::info!("Starting restore pipeline for database: {} from backup: {}", db_id, backup_id);

        // Validate that the backup_id explicitly belongs to the db_id
        if !backup_id.starts_with(&format!("{}/", db_id)) {
            return Err(anyhow!("Security fault: Attempting to restore a backup that does not belong to DB ID '{}'", db_id));
        }

        let download_stream = self.uploader.download(backup_id).await?;
        let decrypted_stream = self.encryptor.decrypt_stream(download_stream);
        let mut decompressed_stream = self.compressor.decompress_stream(decrypted_stream);

        if let Some(file_path) = to_file {
            let mut file = tokio::fs::File::create(&file_path).await?;
            tokio::io::copy(&mut decompressed_stream, &mut file).await?;
            tracing::info!("Restore completed successfully for database: {}. Output written to file: {}", db_id, file_path);
        } else {
            self.extractor.inject(decompressed_stream).await?;
            tracing::info!("Restore completed successfully for database: {}", db_id);
        }

        Ok(())
    }
}
