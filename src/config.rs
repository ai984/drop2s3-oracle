use crate::portable_crypto::EncryptedCredentials;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

/// Main configuration structure matching spec section 5.3
#[derive(Deserialize, Serialize, Clone)]
pub struct Config {
    pub oracle: OracleConfig,
    pub app: AppConfig,
    pub advanced: AdvancedConfig,
    #[serde(default)]
    pub credentials: Option<EncryptedCredentials>,
}

impl std::fmt::Debug for Config {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Config")
            .field("oracle", &self.oracle)
            .field("app", &self.app)
            .field("advanced", &self.advanced)
            .field("credentials", &self.credentials.as_ref().map(|_| "[ENCRYPTED]"))
            .finish()
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct OracleConfig {
    pub endpoint: String,
    pub bucket: String,
    pub namespace: String,
    pub region: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct AppConfig {
    pub auto_copy_link: bool,
    pub auto_start: bool,
    #[serde(default)]
    pub window_x: Option<f32>,
    #[serde(default)]
    pub window_y: Option<f32>,
}

/// Advanced upload configuration
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct AdvancedConfig {
    pub parallel_uploads: u32,
    pub multipart_threshold_mb: u32,
    pub multipart_chunk_mb: u32,
}

impl Config {
    /// Load configuration from TOML file
    ///
    /// # Arguments
    /// * `path` - Path to config.toml file
    ///
    /// # Returns
    /// * `Ok(Config)` - Successfully parsed and validated config
    /// * `Err` - File not found, parse error, or validation error
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();
        let content = fs::read_to_string(path)
            .with_context(|| format!("Failed to read config file: {}", path.display()))?;

        let config: Config = toml::from_str(&content)
            .with_context(|| format!("Failed to parse TOML from: {}", path.display()))?;

        config.validate()?;

        Ok(config)
    }

    pub fn save<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let path = path.as_ref();
        let content = toml::to_string_pretty(self).context("Failed to serialize config to TOML")?;

        fs::write(path, content)
            .with_context(|| format!("Failed to write config file: {}", path.display()))?;

        Ok(())
    }

    /// Check if credentials are configured
    pub fn has_credentials(&self) -> bool {
        self.credentials.is_some()
    }

    /// Validate required fields are non-empty
    fn validate(&self) -> Result<()> {
        if self.oracle.endpoint.trim().is_empty() {
            anyhow::bail!("oracle.endpoint cannot be empty");
        }

        if self.oracle.bucket.trim().is_empty() {
            anyhow::bail!("oracle.bucket cannot be empty");
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_parse_valid_config() {
        let config_content = r#"
[oracle]
endpoint = "https://test.objectstorage.eu-frankfurt-1.oraclecloud.com"
bucket = "test-bucket"
namespace = "test-namespace"
region = "eu-frankfurt-1"

[app]
auto_copy_link = true
auto_start = false

[advanced]
parallel_uploads = 3
multipart_threshold_mb = 5
multipart_chunk_mb = 5
"#;

        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(config_content.as_bytes()).unwrap();
        temp_file.flush().unwrap();

        let config = Config::load(temp_file.path()).unwrap();

        assert_eq!(
            config.oracle.endpoint,
            "https://test.objectstorage.eu-frankfurt-1.oraclecloud.com"
        );
        assert_eq!(config.oracle.bucket, "test-bucket");
        assert_eq!(config.oracle.region, "eu-frankfurt-1");
        assert!(config.app.auto_copy_link);
        assert!(!config.app.auto_start);
        assert_eq!(config.advanced.parallel_uploads, 3);
        assert_eq!(config.advanced.multipart_threshold_mb, 5);
        assert_eq!(config.advanced.multipart_chunk_mb, 5);
    }

    #[test]
    fn test_validate_required_fields() {
        // Test empty endpoint
        let config_content = r#"
[oracle]
endpoint = ""
bucket = "test-bucket"
namespace = "test-namespace"
region = "eu-frankfurt-1"

[app]
auto_copy_link = true
auto_start = false

[advanced]
parallel_uploads = 3
multipart_threshold_mb = 5
multipart_chunk_mb = 5
"#;

        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(config_content.as_bytes()).unwrap();
        temp_file.flush().unwrap();

        let result = Config::load(temp_file.path());
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("oracle.endpoint cannot be empty"));

        // Test empty bucket
        let config_content = r#"
[oracle]
endpoint = "https://test.objectstorage.eu-frankfurt-1.oraclecloud.com"
bucket = ""
namespace = "test-namespace"
region = "eu-frankfurt-1"

[app]
auto_copy_link = true
auto_start = false

[advanced]
parallel_uploads = 3
multipart_threshold_mb = 5
multipart_chunk_mb = 5
"#;

        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(config_content.as_bytes()).unwrap();
        temp_file.flush().unwrap();

        let result = Config::load(temp_file.path());
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("oracle.bucket cannot be empty"));
    }

    #[test]
    fn test_malformed_toml_error() {
        let config_content = r#"
[oracle
endpoint = "https://test.objectstorage.eu-frankfurt-1.oraclecloud.com"
bucket = "test-bucket"
"#;

        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(config_content.as_bytes()).unwrap();
        temp_file.flush().unwrap();

        let result = Config::load(temp_file.path());
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Failed to parse TOML"));
    }
}
