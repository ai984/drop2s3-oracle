use anyhow::Result;
use serde::Deserialize;

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
}

impl UpdateManager {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }

    /// Check GitHub Releases API for newer version.
    /// Returns Some(version) if update available, None otherwise.
    pub async fn check_for_updates(&self) -> Result<Option<String>> {
        let url = format!(
            "https://api.github.com/repos/{}/releases/latest",
            GITHUB_REPO
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

    /// Download update to Drop2S3_new.exe in background.
    pub async fn download_update(&self, version: &str) -> Result<()> {
        let url = format!(
            "https://api.github.com/repos/{}/releases/tags/v{}",
            GITHUB_REPO, version
        );

        let release: Release = self
            .client
            .get(&url)
            .header("User-Agent", "Drop2S3")
            .send()
            .await?
            .json()
            .await?;

        // Find .exe asset
        let asset = release
            .assets
            .iter()
            .find(|a| a.name.ends_with(".exe"))
            .ok_or_else(|| anyhow::anyhow!("No .exe found in release"))?;

        // Download to Drop2S3_new.exe
        let bytes = self
            .client
            .get(&asset.browser_download_url)
            .send()
            .await?
            .bytes()
            .await?;

        tokio::fs::write("Drop2S3_new.exe", bytes).await?;

        Ok(())
    }

    /// Compare semver versions. Returns true if latest > current.
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

    /// Apply update on next startup by replacing Drop2S3.exe with Drop2S3_new.exe.
    /// Should be called at application startup before main logic.
    pub fn apply_update_on_restart() -> Result<()> {
        if std::path::Path::new("Drop2S3_new.exe").exists() {
            // Windows-safe file replacement:
            // 1. Rename current .exe to .old (will be deleted later)
            // 2. Rename new .exe to current
            if std::path::Path::new("Drop2S3.exe").exists() {
                std::fs::rename("Drop2S3.exe", "Drop2S3_old.exe")?;
            }
            std::fs::rename("Drop2S3_new.exe", "Drop2S3.exe")?;

            // Clean up old version
            if std::path::Path::new("Drop2S3_old.exe").exists() {
                let _ = std::fs::remove_file("Drop2S3_old.exe");
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
        // Verify CURRENT_VERSION is set correctly from Cargo.toml
        assert!(!CURRENT_VERSION.is_empty());
        assert!(CURRENT_VERSION.contains('.'));
    }
}
