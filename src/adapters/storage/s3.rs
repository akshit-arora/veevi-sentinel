use crate::ports::storage::StorageOperator;
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use aws_config::Region;
use aws_sdk_s3::{config::Credentials, config::Builder as S3ConfigBuilder, Client};
use aws_sdk_s3::primitives::ByteStream;
use tokio::io::AsyncRead;
use tempfile::NamedTempFile;

pub struct S3Adapter {
    client: Client,
    bucket: String,
}

impl S3Adapter {
    pub fn new(
        bucket: &str,
        region: &str,
        endpoint: Option<&str>,
        access_key: &str,
        secret_key: &str,
    ) -> Self {
        let credentials = Credentials::new(access_key, secret_key, None, None, "veevi-sentinel");
        
        let mut builder = S3ConfigBuilder::new()
            .behavior_version(aws_sdk_s3::config::BehaviorVersion::latest())
            .region(Region::new(region.to_string()))
            .credentials_provider(credentials);

        if let Some(ep) = endpoint {
            builder = builder.endpoint_url(ep);
            builder = builder.force_path_style(true); // Usually required for R2 or MinIO
        }

        let config = builder.build();
        let client = Client::from_conf(config);

        Self {
            client,
            bucket: bucket.to_string(),
        }
    }
}

#[async_trait]
impl StorageOperator for S3Adapter {
    async fn upload(&self, key: &str, mut input: Box<dyn AsyncRead + Unpin + Send>) -> Result<()> {
        // Stream directly to a secure temporary file using tempfile (0600 permissions, immune to symlink attacks)
        let temp_file = tokio::task::spawn_blocking(|| NamedTempFile::new()).await?
            .map_err(|e| anyhow!("Failed to create secure temp file: {}", e))?;
        let temp_path = temp_file.path().to_owned();

        let mut file = tokio::fs::File::create(&temp_path).await?;
        tokio::io::copy(&mut input, &mut file).await?;
        
        let byte_stream = ByteStream::from_path(&temp_path).await
            .map_err(|e| anyhow!("Failed to read temp file: {:?}", e))?;

        let _result = self.client
            .put_object()
            .bucket(&self.bucket)
            .key(key)
            .body(byte_stream)
            .send()
            .await;
            
        // Explicitly close and drop the temp_file handle here to unlink it securely
        drop(temp_file);
        
        _result.map_err(|e| anyhow!("S3 upload failed: {:?}", e))?;
        Ok(())
    }

    async fn download(&self, key: &str) -> Result<Box<dyn AsyncRead + Unpin + Send>> {
        let resp = self.client
            .get_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await
            .map_err(|e| anyhow!("S3 download failed: {:?}", e))?;

        let body = resp.body.into_async_read();
        Ok(Box::new(body))
    }

    async fn list_backups(&self, db_id: &str) -> Result<Vec<String>> {
        // Assuming keys are structured as `db_id/timestamp.backup`
        let prefix = format!("{}/", db_id);
        
        let resp = self.client
            .list_objects_v2()
            .bucket(&self.bucket)
            .prefix(&prefix)
            .send()
            .await
            .map_err(|e| anyhow!("S3 list failed: {:?}", e))?;

        let mut keys = Vec::new();
        for obj in resp.contents() {
            if let Some(key) = obj.key() {
                keys.push(key.to_string());
            }
        }
        
        // Sort keys to easily find oldest/newest
        keys.sort();
        Ok(keys)
    }

    async fn prune_backups(&self, db_id: &str, keep_last_n: usize) -> Result<()> {
        let keys = self.list_backups(db_id).await?;
        
        if keys.len() <= keep_last_n {
            return Ok(()); // Nothing to prune
        }

        let to_delete = keys.len() - keep_last_n;
        for key in keys.iter().take(to_delete) {
            self.client
                .delete_object()
                .bucket(&self.bucket)
                .key(key)
                .send()
                .await
                .map_err(|e| anyhow!("Failed to delete {}: {:?}", key, e))?;
        }

        Ok(())
    }
}
