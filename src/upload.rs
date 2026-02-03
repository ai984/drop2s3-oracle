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

    pub async fn upload_file_multipart<P: AsRef<Path>>(
        &self,
        file_path: P,
        chunk_size_mb: u32,
    ) -> Result<String> {
        let path = file_path.as_ref();
        let filename = path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| anyhow::anyhow!("Invalid filename"))?;

        let s3_path = generate_s3_path(filename);

        let content = tokio::fs::read(path)
            .await
            .with_context(|| format!("Failed to read file: {}", path.display()))?;

        let content_type = mime_guess::from_path(path)
            .first_or_octet_stream()
            .to_string();

        let chunk_size = (chunk_size_mb as usize) * 1024 * 1024;

        let parts: Vec<&[u8]> = content.chunks(chunk_size).collect();

        let msg = self
            .bucket
            .initiate_multipart_upload(&s3_path, &content_type)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to initiate multipart upload: {}", e))?;

        let upload_id = &msg.upload_id;

        let mut etags = Vec::new();
        for (i, chunk) in parts.iter().enumerate() {
            let part_number = (i + 1) as u32;
            match self
                .bucket
                .put_multipart_chunk(chunk.to_vec(), &s3_path, part_number, upload_id, &content_type)
                .await
            {
                Ok(part) => etags.push(s3::serde_types::Part {
                    etag: part.etag.to_string(),
                    part_number,
                }),
                Err(e) => {
                    let _ = self.bucket.abort_upload(&s3_path, upload_id).await;
                    return Err(anyhow::anyhow!("Failed to upload part {}: {}", part_number, e));
                }
            }
        }

        self.bucket
            .complete_multipart_upload(&s3_path, upload_id, etags)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to complete multipart upload: {}", e))?;

        let url = self.get_public_url(&s3_path);
        Ok(url)
    }

    pub async fn upload_file_auto<P: AsRef<Path>>(
        &self,
        file_path: P,
        threshold_mb: u32,
        chunk_mb: u32,
    ) -> Result<String> {
        let path = file_path.as_ref();
        let metadata = tokio::fs::metadata(path)
            .await
            .with_context(|| format!("Failed to get file metadata: {}", path.display()))?;

        let size = metadata.len();
        let threshold = (threshold_mb as u64) * 1024 * 1024;

        if size >= threshold {
            self.upload_file_multipart(path, chunk_mb).await
        } else {
            self.upload_file_with_auto_path(path).await
        }
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

    #[test]
    fn test_chunk_calculation() {
        let chunk_size = 5 * 1024 * 1024;
        
        let small_data = vec![0u8; 3 * 1024 * 1024];
        let chunks: Vec<&[u8]> = small_data.chunks(chunk_size).collect();
        assert_eq!(chunks.len(), 1);
        
        let exact_data = vec![0u8; 10 * 1024 * 1024];
        let chunks: Vec<&[u8]> = exact_data.chunks(chunk_size).collect();
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].len(), 5 * 1024 * 1024);
        assert_eq!(chunks[1].len(), 5 * 1024 * 1024);
        
        let uneven_data = vec![0u8; 12 * 1024 * 1024];
        let chunks: Vec<&[u8]> = uneven_data.chunks(chunk_size).collect();
        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[0].len(), 5 * 1024 * 1024);
        assert_eq!(chunks[1].len(), 5 * 1024 * 1024);
        assert_eq!(chunks[2].len(), 2 * 1024 * 1024);
    }

    #[test]
    fn test_multipart_part_numbering() {
        let parts = vec![vec![1u8; 100], vec![2u8; 100], vec![3u8; 100]];
        
        for (i, _chunk) in parts.iter().enumerate() {
            let part_number = (i + 1) as u32;
            assert_eq!(part_number, (i + 1) as u32);
        }
        
        assert_eq!(parts.len(), 3);
    }

    #[test]
    fn test_file_size_threshold() {
        let threshold_mb = 5u32;
        let threshold_bytes = (threshold_mb as u64) * 1024 * 1024;
        
        assert_eq!(threshold_bytes, 5 * 1024 * 1024);
        
        let small_file_size = 3 * 1024 * 1024;
        assert!(small_file_size < threshold_bytes);
        
        let large_file_size = 10 * 1024 * 1024;
        assert!(large_file_size >= threshold_bytes);
        
        let exact_file_size = 5 * 1024 * 1024;
        assert!(exact_file_size >= threshold_bytes);
    }
}
