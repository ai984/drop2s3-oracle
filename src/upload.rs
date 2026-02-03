use anyhow::{Context, Result};
use s3::creds::Credentials;
use s3::{Bucket, Region};
use std::path::Path;

use crate::config::Config;
use crate::crypto;

pub struct S3Client {
    bucket: Box<Bucket>,
}

impl S3Client {
    pub async fn new(config: &Config) -> Result<Self> {
        let access_key = crypto::decrypt(&config.oracle.access_key)
            .context("Failed to decrypt access_key")?;
        let secret_key = crypto::decrypt(&config.oracle.secret_key)
            .context("Failed to decrypt secret_key")?;

        let credentials = Credentials::new(
            Some(&access_key),
            Some(&secret_key),
            None,
            None,
            None,
        )
        .map_err(|e| anyhow::anyhow!("Failed to create S3 credentials: {}", e))?;

        let region = Region::Custom {
            region: config.oracle.region.clone(),
            endpoint: config.oracle.endpoint.clone(),
        };

        let bucket = Bucket::new(&config.oracle.bucket, region, credentials)
            .map_err(|e| anyhow::anyhow!("Failed to create S3 bucket: {}", e))?
            .with_path_style();

        Ok(Self { bucket })
    }

    pub async fn test_connection(&self) -> Result<()> {
        self.bucket
            .list("/".to_string(), Some("/".to_string()))
            .await
            .map_err(|e| anyhow::anyhow!("Connection test failed: {}", e))?;
        Ok(())
    }

    pub async fn upload_file(&self, local_path: &Path, remote_key: &str) -> Result<String> {
        let content = tokio::fs::read(local_path)
            .await
            .with_context(|| format!("Failed to read file: {}", local_path.display()))?;

        let content_type = mime_guess::from_path(local_path)
            .first_or_octet_stream()
            .to_string();

        self.bucket
            .put_object_with_content_type(remote_key, &content, &content_type)
            .await
            .map_err(|e| anyhow::anyhow!("Upload failed: {}", e))?;

        let url = self.get_public_url(remote_key);
        Ok(url)
    }

    pub async fn upload_bytes(&self, data: &[u8], remote_key: &str, content_type: &str) -> Result<String> {
        self.bucket
            .put_object_with_content_type(remote_key, data, content_type)
            .await
            .map_err(|e| anyhow::anyhow!("Upload failed: {}", e))?;

        let url = self.get_public_url(remote_key);
        Ok(url)
    }

    fn get_public_url(&self, key: &str) -> String {
        format!(
            "{}/{}/{}",
            self.bucket.url(),
            self.bucket.name(),
            key
        )
    }
}

pub struct UploadManager {
    client: Option<S3Client>,
}

impl UploadManager {
    pub fn new() -> Self {
        UploadManager { client: None }
    }

    pub async fn initialize(&mut self, config: &Config) -> Result<()> {
        let client = S3Client::new(config).await?;
        self.client = Some(client);
        Ok(())
    }

    pub fn is_initialized(&self) -> bool {
        self.client.is_some()
    }

    pub async fn upload(&self, local_path: &Path, remote_key: &str) -> Result<String> {
        let client = self.client.as_ref()
            .ok_or_else(|| anyhow::anyhow!("S3Client not initialized"))?;
        client.upload_file(local_path, remote_key).await
    }
}

impl Default for UploadManager {
    fn default() -> Self {
        Self::new()
    }
}
