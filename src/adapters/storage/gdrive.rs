use crate::ports::storage::StorageOperator;
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use google_drive3::{api::File, hyper, hyper_rustls, DriveHub};
use tokio::io::{AsyncRead, AsyncWriteExt};
use yup_oauth2::DeviceFlowAuthenticator;
use tempfile::NamedTempFile;

pub struct GDriveAdapter {
    hub: DriveHub<hyper_rustls::HttpsConnector<hyper::client::HttpConnector>>,
}

impl GDriveAdapter {
    pub async fn new(client_secret_file: &str, token_cache_file: &str) -> Result<Self> {
        let secret_content = tokio::fs::read_to_string(client_secret_file)
            .await
            .map_err(|e| anyhow!("Failed to read GDrive client secret file: {}", e))?;
            
        // Google omits `redirect_uris` for 'TVs and Limited Input' clients, which breaks `yup_oauth2` strict parsing.
        let mut json: serde_json::Value = serde_json::from_str(&secret_content)
            .map_err(|e| anyhow!("Invalid JSON in client secret: {}", e))?;
            
        if let Some(installed) = json.get_mut("installed") {
            if installed.get("redirect_uris").is_none() {
                installed["redirect_uris"] = serde_json::json!(["http://localhost"]);
            }
        }
        
        let console_secret: yup_oauth2::ConsoleApplicationSecret = serde_json::from_value(json)
            .map_err(|e| anyhow!("Failed to parse Google OAuth2 client secret format: {}", e))?;
            
        let secret = console_secret.installed.or(console_secret.web)
            .ok_or_else(|| anyhow!("Client secret JSON must contain 'installed' or 'web' block"))?;

        let auth = DeviceFlowAuthenticator::builder(secret)
            .persist_tokens_to_disk(token_cache_file)
            .build()
            .await
            .map_err(|e| anyhow!("Failed to create device flow authenticator: {}", e))?;

        let connector = hyper_rustls::HttpsConnectorBuilder::new()
            .with_native_roots()
            .map_err(|e| anyhow!("Failed to get native roots for TLS: {}", e))?
            .https_or_http()
            .enable_http1()
            .build();

        let client = hyper::Client::builder().build(connector);
        let hub = DriveHub::new(client, auth);

        Ok(Self {
            hub,
        })
    }
    
    // Helper to get or create our secure backup folder since `drive.file` can only see what it creates
    async fn get_or_create_folder(&self) -> Result<String> {
        let query = "name='VeeviSentinelBackups' and mimeType='application/vnd.google-apps.folder' and trashed=false";
        let (_, list) = self.hub.files().list().q(query)
            .add_scope("https://www.googleapis.com/auth/drive.file")
            .doit().await
            .map_err(|e| anyhow!("GDrive folder search failed: {:?}", e))?;
            
        if let Some(files) = list.files {
            if let Some(f) = files.into_iter().next() {
                return Ok(f.id.unwrap());
            }
        }
        
        let req = File {
            name: Some("VeeviSentinelBackups".to_string()),
            mime_type: Some("application/vnd.google-apps.folder".to_string()),
            ..Default::default()
        };
        
        let (_, f) = self.hub.files().create(req)
            .add_scope("https://www.googleapis.com/auth/drive.file")
            .upload(std::io::Cursor::new(Vec::new()), "application/vnd.google-apps.folder".parse().unwrap())
            .await
            .map_err(|e| anyhow!("Failed to create backup folder: {:?}", e))?;
            
        Ok(f.id.unwrap())
    }
    
    // Helper to find file ID by name and parent folder
    async fn find_file_id(&self, name: &str, folder_id: &str) -> Result<Option<String>> {
        let safe_name = name.replace("'", "\\'");
        let query = format!("name='{}' and '{}' in parents and trashed=false", safe_name, folder_id);
        let (_, list) = self.hub.files().list().q(&query)
            .add_scope("https://www.googleapis.com/auth/drive.file")
            .doit().await
            .map_err(|e| anyhow!("GDrive search failed: {:?}", e))?;
            
        if let Some(files) = list.files {
            if let Some(f) = files.into_iter().next() {
                return Ok(f.id);
            }
        }
        Ok(None)
    }
}

#[async_trait]
impl StorageOperator for GDriveAdapter {
    async fn upload(&self, key: &str, mut input: Box<dyn AsyncRead + Unpin + Send>) -> Result<()> {
        // Stream into secure temporary disk to avoid OOM and local symlink attacks
        let temp_file = tokio::task::spawn_blocking(|| NamedTempFile::new()).await?
            .map_err(|e| anyhow!("Failed to create secure temp file: {}", e))?;
        let temp_path = temp_file.path().to_owned();

        let mut file = tokio::fs::File::create(&temp_path).await?;
        tokio::io::copy(&mut input, &mut file).await?;

        let folder_id = self.get_or_create_folder().await?;
        let req = File {
            name: Some(key.to_string()),
            parents: Some(vec![folder_id]),
            ..Default::default()
        };

        let result = self.hub
            .files()
            .create(req)
            .add_scope("https://www.googleapis.com/auth/drive.file")
            .upload_resumable(
                std::fs::File::open(&temp_path)?,
                "application/octet-stream".parse().unwrap(),
            )
            .await;

        drop(temp_file);

        match result {
            Ok(_) => Ok(()),
            Err(e) => Err(anyhow!("GDrive upload failed: {:?}", e)),
        }
    }

    async fn download(&self, key: &str) -> Result<Box<dyn AsyncRead + Unpin + Send>> {
        let folder_id = self.get_or_create_folder().await?;
        let file_id = self.find_file_id(key, &folder_id).await?
            .ok_or_else(|| anyhow!("File {} not found in GDrive folder", key))?;
            
        let (resp, _) = self.hub.files().get(&file_id).param("alt", "media")
            .add_scope("https://www.googleapis.com/auth/drive.file")
            .doit().await
            .map_err(|e| anyhow!("GDrive download failed: {:?}", e))?;
            
        // For MVP, we spool it to disk securely to prevent OOM
        let temp_file = tokio::task::spawn_blocking(|| NamedTempFile::new()).await?
            .map_err(|e| anyhow!("Failed to create secure temp file: {}", e))?;
        let temp_path = temp_file.path().to_owned();
        
        let body_bytes = hyper::body::to_bytes(resp.into_body()).await
            .map_err(|e| anyhow!("Failed to read GDrive response: {}", e))?;
            
        let mut file = tokio::fs::File::create(&temp_path).await?;
        file.write_all(&body_bytes).await?;
        
        let file = tokio::fs::File::open(&temp_path).await?;
        
        // Remove file securely. Linux allows reading unlinked files natively.
        drop(temp_file);
        
        Ok(Box::new(file))
    }

    async fn list_backups(&self, db_id: &str) -> Result<Vec<String>> {
        let folder_id = self.get_or_create_folder().await?;
        let safe_db_id = db_id.replace("'", "\\'");
        let query = format!("name contains '{}' and '{}' in parents and trashed=false", safe_db_id, folder_id);
        let (_, list) = self.hub.files().list().q(&query)
            .add_scope("https://www.googleapis.com/auth/drive.file")
            .doit().await
            .map_err(|e| anyhow!("GDrive list failed: {:?}", e))?;
            
        let mut keys = Vec::new();
        if let Some(files) = list.files {
            for f in files {
                if let Some(name) = f.name {
                    if name.starts_with(&format!("{}/", db_id)) || name.starts_with(db_id) {
                        keys.push(name);
                    }
                }
            }
        }
        
        keys.sort();
        Ok(keys)
    }

    async fn prune_backups(&self, db_id: &str, keep_last_n: usize) -> Result<()> {
        let keys = self.list_backups(db_id).await?;
        if keys.len() <= keep_last_n {
            return Ok(());
        }

        let to_delete = keys.len() - keep_last_n;
        let folder_id = self.get_or_create_folder().await?;
        for key in keys.iter().take(to_delete) {
            if let Some(id) = self.find_file_id(key, &folder_id).await? {
                if let Err(e) = self.hub.files().delete(&id)
                    .add_scope("https://www.googleapis.com/auth/drive.file")
                    .doit().await 
                {
                    tracing::error!("Failed to delete {} from GDrive: {:?}", key, e);
                }
            }
        }

        Ok(())
    }
}
