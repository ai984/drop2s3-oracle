use anyhow::{Context, Result};
use s3::creds::Credentials;
use s3::{Bucket, Region};
use std::path::Path;
use uuid::Uuid;

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

    pub async fn upload_file_with_auto_path(&self, local_path: &Path) -> Result<String> {
        let filename = local_path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| anyhow::anyhow!("Invalid filename"))?;

        let s3_path = generate_s3_path(filename);

        let content = tokio::fs::read(local_path)
            .await
            .with_context(|| format!("Failed to read file: {}", local_path.display()))?;

        let content_type = mime_guess::from_path(local_path)
            .first_or_octet_stream()
            .to_string();

        self.bucket
            .put_object_with_content_type(&s3_path, &content, &content_type)
            .await
            .map_err(|e| anyhow::anyhow!("Upload failed: {}", e))?;

        let url = self.get_public_url(&s3_path);
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

/// Sanitize filename: transliterate Polish chars, lowercase, replace spaces with hyphens
fn sanitize_filename(name: &str) -> String {
    let mut result = String::with_capacity(name.len());
    
    for ch in name.chars() {
        let replacement = match ch {
            'ż' | 'Ż' => "z",
            'ó' | 'Ó' => "o",
            'ł' | 'Ł' => "l",
            'ą' | 'Ą' => "a",
            'ę' | 'Ę' => "e",
            'ć' | 'Ć' => "c",
            'ń' | 'Ń' => "n",
            'ś' | 'Ś' => "s",
            'ź' | 'Ź' => "z",
            ' ' => "-",
            c if c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_' => {
                result.push(c.to_ascii_lowercase());
                continue;
            }
            _ => continue,
        };
        result.push_str(replacement);
    }
    
    result
}

/// Generate 16-character hex UUID
fn generate_uuid16() -> String {
    Uuid::new_v4()
        .to_string()
        .replace("-", "")
        .chars()
        .take(16)
        .collect()
}

/// Generate S3 path: YYYY-MM-DD/UUID16/sanitized-filename
fn generate_s3_path(filename: &str) -> String {
    let date = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let uuid = generate_uuid16();
    let sanitized = sanitize_filename(filename);
    format!("{}/{}/{}", date, uuid, sanitized)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_filename() {
        assert_eq!(sanitize_filename("żółć test.PNG"), "zolc-test.png");
        assert_eq!(sanitize_filename("Ąęćńśźż.txt"), "aecnszz.txt");
        assert_eq!(sanitize_filename("file name.pdf"), "file-name.pdf");
        assert_eq!(sanitize_filename("UPPERCASE.TXT"), "uppercase.txt");
        assert_eq!(sanitize_filename("special!@#$%chars.doc"), "specialchars.doc");
        assert_eq!(sanitize_filename("under_score-dash.txt"), "under_score-dash.txt");
    }

    #[test]
    fn test_generate_uuid16() {
        let uuid1 = generate_uuid16();
        let uuid2 = generate_uuid16();
        
        assert_eq!(uuid1.len(), 16);
        assert_eq!(uuid2.len(), 16);
        
        assert!(uuid1.chars().all(|c| c.is_ascii_hexdigit()));
        assert!(uuid2.chars().all(|c| c.is_ascii_hexdigit()));
        
        assert_ne!(uuid1, uuid2);
    }

    #[test]
    fn test_generate_s3_path() {
        let path = generate_s3_path("żółć test.PNG");
        
        let parts: Vec<&str> = path.split('/').collect();
        assert_eq!(parts.len(), 3);
        
        assert_eq!(parts[0].len(), 10);
        assert!(parts[0].contains('-'));
        
        assert_eq!(parts[1].len(), 16);
        
        assert_eq!(parts[2], "zolc-test.png");
    }

    #[test]
    fn test_content_type_detection() {
        use std::path::Path;
        
        let png_type = mime_guess::from_path(Path::new("test.png"))
            .first_or_octet_stream()
            .to_string();
        assert_eq!(png_type, "image/png");
        
        let jpg_type = mime_guess::from_path(Path::new("photo.jpg"))
            .first_or_octet_stream()
            .to_string();
        assert_eq!(jpg_type, "image/jpeg");
        
        let pdf_type = mime_guess::from_path(Path::new("doc.pdf"))
            .first_or_octet_stream()
            .to_string();
        assert_eq!(pdf_type, "application/pdf");
        
        let unknown_type = mime_guess::from_path(Path::new("file.unknown"))
            .first_or_octet_stream()
            .to_string();
        assert_eq!(unknown_type, "application/octet-stream");
    }
}
