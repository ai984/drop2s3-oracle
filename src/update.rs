use anyhow::{Context, Result};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::path::PathBuf;

const GITHUB_REPO: &str = "ai984/drop2s3-oracle";
const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Deserialize)]
struct Release {
    tag_name: String,
    assets: Vec<Asset>,
}

#[derive(Debug, Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
    size: u64,
}

pub struct UpdateManager {
    client: reqwest::Client,
    exe_dir: PathBuf,
}

impl UpdateManager {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
            exe_dir: crate::utils::get_exe_dir(),
        }
    }

    fn new_exe_path(&self) -> PathBuf {
        self.exe_dir.join("drop2s3_new.exe")
    }

    #[cfg(test)]
    fn old_exe_path(&self) -> PathBuf {
        self.exe_dir.join("drop2s3_old.exe")
    }

    fn current_exe_path() -> PathBuf {
        std::env::current_exe().unwrap_or_else(|_| PathBuf::from("drop2s3.exe"))
    }

    pub fn update_ready_to_install(&self) -> bool {
        self.new_exe_path().exists()
    }

    pub async fn check_for_updates(&self) -> Result<Option<String>> {
        if self.update_ready_to_install() {
            tracing::info!("Update already downloaded, skipping check");
            return Ok(None);
        }

        let url = format!(
            "https://api.github.com/repos/{GITHUB_REPO}/releases/latest"
        );

        let response = self
            .client
            .get(&url)
            .header("User-Agent", "Drop2S3")
            .timeout(std::time::Duration::from_secs(10))
            .send()
            .await?;

        if !response.status().is_success() {
            return Ok(None);
        }

        let release: Release = response.json().await?;
        let latest_version = release.tag_name.trim_start_matches('v');

        if Self::is_newer_version(latest_version, CURRENT_VERSION)? {
            tracing::info!("New version available: {} (current: {})", latest_version, CURRENT_VERSION);
            Ok(Some(latest_version.to_string()))
        } else {
            tracing::debug!("No update available (latest: {}, current: {})", latest_version, CURRENT_VERSION);
            Ok(None)
        }
    }

    pub async fn download_update(&self, version: &str) -> Result<()> {
        let url = format!(
            "https://api.github.com/repos/{GITHUB_REPO}/releases/tags/v{version}"
        );

        let release: Release = self
            .client
            .get(&url)
            .header("User-Agent", "Drop2S3")
            .send()
            .await?
            .json()
            .await?;

        let asset = release
            .assets
            .iter()
            .find(|a| a.name.ends_with(".exe") && !a.name.ends_with(".sha256"))
            .ok_or_else(|| anyhow::anyhow!("No .exe found in release"))?;

        let expected_size = asset.size;

        tracing::info!("Downloading update from: {}", asset.browser_download_url);

        let bytes = self
            .client
            .get(&asset.browser_download_url)
            .send()
            .await?
            .bytes()
            .await?;

        // Verify file size matches GitHub API metadata
        if bytes.len() as u64 != expected_size {
            anyhow::bail!(
                "Size mismatch: expected {} bytes, got {} bytes",
                expected_size,
                bytes.len()
            );
        }

        // Try to verify SHA256 checksum if available
        let sha256_asset = release
            .assets
            .iter()
            .find(|a| a.name.ends_with(".sha256"));

        if let Some(sha256_asset) = sha256_asset {
            match self.verify_sha256(&bytes, &sha256_asset.browser_download_url).await {
                Ok(()) => tracing::info!("SHA256 checksum verified"),
                Err(e) => {
                    anyhow::bail!("SHA256 verification failed: {}", e);
                }
            }
        } else {
            tracing::warn!("No SHA256 checksum file found in release, skipping hash verification");
        }

        let new_exe = self.new_exe_path();
        tracing::info!("Saving update to: {:?} ({} bytes)", new_exe, bytes.len());
        tokio::fs::write(&new_exe, &bytes).await?;

        Ok(())
    }

    async fn verify_sha256(&self, data: &[u8], checksum_url: &str) -> Result<()> {
        let response = self
            .client
            .get(checksum_url)
            .header("User-Agent", "Drop2S3")
            .send()
            .await
            .context("Failed to download SHA256 checksum")?;

        if !response.status().is_success() {
            anyhow::bail!("Failed to download SHA256 checksum: HTTP {}", response.status());
        }

        let checksum_text = response
            .text()
            .await
            .context("Failed to read SHA256 checksum")?;

        // Checksum file format: "hex_hash  filename" or just "hex_hash"
        let expected_hash = checksum_text
            .split_whitespace()
            .next()
            .ok_or_else(|| anyhow::anyhow!("Empty checksum file"))?
            .to_lowercase();

        let mut hasher = Sha256::new();
        hasher.update(data);
        let actual_hash = format!("{:x}", hasher.finalize());

        if actual_hash != expected_hash {
            anyhow::bail!(
                "SHA256 mismatch: expected {}, got {}",
                expected_hash,
                actual_hash
            );
        }

        Ok(())
    }

    fn is_newer_version(latest: &str, current: &str) -> Result<bool> {
        let latest_parts: Vec<u32> = latest
            .split('.')
            .filter_map(|s| s.parse().ok())
            .collect();
        let current_parts: Vec<u32> = current
            .split('.')
            .filter_map(|s| s.parse().ok())
            .collect();

        Ok(latest_parts > current_parts)
    }

    pub fn apply_update_on_shutdown() -> Result<bool> {
        let exe_dir = crate::utils::get_exe_dir();
        let new_exe = exe_dir.join("drop2s3_new.exe");
        let old_exe = exe_dir.join("drop2s3_old.exe");
        let current_exe = Self::current_exe_path();

        if !new_exe.exists() {
            return Ok(false);
        }

        tracing::info!("Applying update on shutdown...");
        tracing::info!("  Current: {:?}", current_exe);
        tracing::info!("  New: {:?}", new_exe);
        tracing::info!("  Old: {:?}", old_exe);

        if current_exe.exists() {
            tracing::info!("Renaming current exe to old...");
            std::fs::rename(&current_exe, &old_exe)?;
        }

        tracing::info!("Renaming new exe to current...");
        std::fs::rename(&new_exe, &current_exe)?;

        tracing::info!("Update applied successfully! Next start will use new version.");
        Ok(true)
    }

    pub fn cleanup_old_version() {
        let exe_dir = crate::utils::get_exe_dir();
        let old_exe = exe_dir.join("drop2s3_old.exe");

        if old_exe.exists() {
            tracing::info!("Cleaning up old version: {:?}", old_exe);
            match std::fs::remove_file(&old_exe) {
                Ok(()) => tracing::info!("Old version removed successfully"),
                Err(e) => tracing::warn!("Failed to remove old version: {}", e),
            }
        }
    }
}

impl Default for UpdateManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_comparison_newer() {
        assert!(UpdateManager::is_newer_version("0.2.0", "0.1.0").unwrap());
        assert!(UpdateManager::is_newer_version("1.0.0", "0.9.9").unwrap());
        assert!(UpdateManager::is_newer_version("0.1.1", "0.1.0").unwrap());
    }

    #[test]
    fn test_version_comparison_same() {
        assert!(!UpdateManager::is_newer_version("0.1.0", "0.1.0").unwrap());
        assert!(!UpdateManager::is_newer_version("1.2.3", "1.2.3").unwrap());
    }

    #[test]
    fn test_version_comparison_older() {
        assert!(!UpdateManager::is_newer_version("0.1.0", "0.2.0").unwrap());
        assert!(!UpdateManager::is_newer_version("0.9.9", "1.0.0").unwrap());
    }

    #[test]
    fn test_current_version_constant() {
        assert!(!CURRENT_VERSION.is_empty());
        assert!(CURRENT_VERSION.contains('.'));
    }

    #[test]
    fn test_exe_paths() {
        let manager = UpdateManager::new();
        let new_path = manager.new_exe_path();
        let old_path = manager.old_exe_path();
        
        assert!(new_path.to_string_lossy().contains("drop2s3_new.exe"));
        assert!(old_path.to_string_lossy().contains("drop2s3_old.exe"));
    }
}
