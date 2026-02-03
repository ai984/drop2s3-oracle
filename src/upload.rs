use anyhow::{Context, Result};
use s3::creds::Credentials;
use s3::{Bucket, Region};
use std::path::{Path, PathBuf};
use std::time::Duration;
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

/// Upload status tracking
#[derive(Debug, Clone, PartialEq)]
pub enum UploadStatus {
    Queued,
    Uploading,
    Completed,
    Failed(String),
}

/// Progress information for a single file upload
#[derive(Debug, Clone)]
pub struct UploadProgress {
    pub file_id: String,
    pub filename: String,
    pub bytes_uploaded: u64,
    pub total_bytes: u64,
    pub status: UploadStatus,
}

/// Manages upload queue with parallel processing and progress tracking
pub struct UploadManager {
    s3_client: S3Client,
    parallel_limit: usize,
    max_retries: u32,
    progress_tx: tokio::sync::mpsc::UnboundedSender<UploadProgress>,
}

impl UploadManager {
    pub fn new(
        s3_client: S3Client,
        parallel_limit: usize,
        max_retries: u32,
    ) -> (Self, tokio::sync::mpsc::UnboundedReceiver<UploadProgress>) {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        (
            Self {
                s3_client,
                parallel_limit,
                max_retries,
                progress_tx: tx,
            },
            rx,
        )
    }

    pub async fn upload_files(&self, files: Vec<PathBuf>) -> Result<Vec<String>> {
        use futures::stream::{self, StreamExt};

        let results = stream::iter(files)
            .map(|file| self.upload_with_retry(file))
            .buffer_unordered(self.parallel_limit)
            .collect::<Vec<_>>()
            .await;

        results.into_iter().collect()
    }

    async fn upload_with_retry(&self, file: PathBuf) -> Result<String> {
        let mut attempts = 0;
        loop {
            match self.upload_with_progress(file.clone()).await {
                Ok(url) => return Ok(url),
                Err(e) if attempts < self.max_retries => {
                    attempts += 1;
                    let delay_secs = 2_u64.pow(attempts);
                    tokio::time::sleep(Duration::from_secs(delay_secs)).await;
                }
                Err(e) => {
                    let file_id = Uuid::new_v4().to_string();
                    let filename = file
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("unknown")
                        .to_string();
                    
                    let _ = self.progress_tx.send(UploadProgress {
                        file_id,
                        filename,
                        bytes_uploaded: 0,
                        total_bytes: 0,
                        status: UploadStatus::Failed(e.to_string()),
                    });
                    
                    return Err(e);
                }
            }
        }
    }

    async fn upload_with_progress(&self, file: PathBuf) -> Result<String> {
        let file_id = Uuid::new_v4().to_string();
        let filename = file
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| anyhow::anyhow!("Invalid filename"))?
            .to_string();

        let total_bytes = tokio::fs::metadata(&file)
            .await
            .with_context(|| format!("Failed to get file metadata: {}", file.display()))?
            .len();

        self.progress_tx
            .send(UploadProgress {
                file_id: file_id.clone(),
                filename: filename.clone(),
                bytes_uploaded: 0,
                total_bytes,
                status: UploadStatus::Queued,
            })
            .map_err(|_| anyhow::anyhow!("Progress channel closed"))?;

        self.progress_tx
            .send(UploadProgress {
                file_id: file_id.clone(),
                filename: filename.clone(),
                bytes_uploaded: 0,
                total_bytes,
                status: UploadStatus::Uploading,
            })
            .map_err(|_| anyhow::anyhow!("Progress channel closed"))?;

        let url = self.s3_client.upload_file_auto(&file, 5, 5).await?;

        self.progress_tx
            .send(UploadProgress {
                file_id,
                filename,
                bytes_uploaded: total_bytes,
                total_bytes,
                status: UploadStatus::Completed,
            })
            .map_err(|_| anyhow::anyhow!("Progress channel closed"))?;

        Ok(url)
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

    #[test]
    fn test_upload_progress_structure() {
        let progress = UploadProgress {
            file_id: "test-id".to_string(),
            filename: "test.txt".to_string(),
            bytes_uploaded: 1024,
            total_bytes: 2048,
            status: UploadStatus::Uploading,
        };

        assert_eq!(progress.file_id, "test-id");
        assert_eq!(progress.filename, "test.txt");
        assert_eq!(progress.bytes_uploaded, 1024);
        assert_eq!(progress.total_bytes, 2048);
        assert_eq!(progress.status, UploadStatus::Uploading);
    }

    #[test]
    fn test_upload_status_variants() {
        let queued = UploadStatus::Queued;
        let uploading = UploadStatus::Uploading;
        let completed = UploadStatus::Completed;
        let failed = UploadStatus::Failed("error".to_string());

        assert_eq!(queued, UploadStatus::Queued);
        assert_eq!(uploading, UploadStatus::Uploading);
        assert_eq!(completed, UploadStatus::Completed);
        
        match failed {
            UploadStatus::Failed(msg) => assert_eq!(msg, "error"),
            _ => panic!("Expected Failed status"),
        }
    }

    #[test]
    fn test_upload_status_clone() {
        let status1 = UploadStatus::Failed("network error".to_string());
        let status2 = status1.clone();

        match (status1, status2) {
            (UploadStatus::Failed(msg1), UploadStatus::Failed(msg2)) => {
                assert_eq!(msg1, msg2);
            }
            _ => panic!("Expected Failed status"),
        }
    }

    #[test]
    fn test_exponential_backoff_calculation() {
        for attempts in 1..=3 {
            let delay_secs = 2_u64.pow(attempts);
            match attempts {
                1 => assert_eq!(delay_secs, 2),
                2 => assert_eq!(delay_secs, 4),
                3 => assert_eq!(delay_secs, 8),
                _ => {}
            }
        }
    }
}
