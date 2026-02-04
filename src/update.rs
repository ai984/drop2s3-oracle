use anyhow::Result;
use serde::Deserialize;
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

    fn old_exe_path(&self) -> PathBuf {
        self.exe_dir.join("drop2s3_old.exe")
    }

    fn current_exe_path() -> PathBuf {
        std::env::current_exe().unwrap_or_else(|_| PathBuf::from("drop2s3.exe"))
    }

    pub fn update_already_downloaded(&self) -> bool {
        self.new_exe_path().exists()
    }

    pub async fn check_for_updates(&self) -> Result<Option<String>> {
        if self.update_already_downloaded() {
            tracing::info!("Update already downloaded, skipping check");
            return Ok(None);
        }

        let url = format!(
            "https://api.github.com/repos/{GITHUB_REPO}/releases/latest"
        );

        let release: Release = self
            .client
            .get(&url)
            .header("User-Agent", "Drop2S3")
            .send()
            .await?
            .json()
            .await?;

        let latest_version = release.tag_name.trim_start_matches('v');

        if Self::is_newer_version(latest_version, CURRENT_VERSION)? {
            Ok(Some(latest_version.to_string()))
        } else {
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
            .find(|a| a.name.ends_with(".exe"))
            .ok_or_else(|| anyhow::anyhow!("No .exe found in release"))?;

        let bytes = self
            .client
            .get(&asset.browser_download_url)
            .send()
            .await?
            .bytes()
            .await?;

        let new_exe = self.new_exe_path();
        tracing::info!("Saving update to: {:?}", new_exe);
        tokio::fs::write(&new_exe, bytes).await?;

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

    pub fn apply_update_on_restart() -> Result<()> {
        let exe_dir = crate::utils::get_exe_dir();
        let new_exe = exe_dir.join("drop2s3_new.exe");
        let old_exe = exe_dir.join("drop2s3_old.exe");
        let current_exe = Self::current_exe_path();

        if old_exe.exists() {
            tracing::info!("Removing old exe: {:?}", old_exe);
            let _ = std::fs::remove_file(&old_exe);
        }

        if new_exe.exists() {
            tracing::info!("Applying update: {:?} -> {:?}", new_exe, current_exe);
            
            if current_exe.exists() {
                std::fs::rename(&current_exe, &old_exe)?;
                tracing::info!("Renamed current to old: {:?}", old_exe);
            }
            
            std::fs::rename(&new_exe, &current_exe)?;
            tracing::info!("Update applied successfully");

            if old_exe.exists() {
                let _ = std::fs::remove_file(&old_exe);
            }
        }
        
        Ok(())
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
