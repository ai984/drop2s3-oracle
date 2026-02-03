use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use crate::portable_crypto::EncryptedCredentials;

/// Main configuration structure matching spec section 5.3
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Config {
    pub oracle: OracleConfig,
    pub app: AppConfig,
    pub advanced: AdvancedConfig,
    #[serde(default)]
    pub credentials: Option<EncryptedCredentials>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct OracleConfig {
    pub endpoint: String,
    pub bucket: String,
    #[serde(default)]
    pub access_key: String,
    #[serde(default)]
    pub secret_key: String,
    pub region: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct AppConfig {
    pub auto_copy_link: bool,
    pub auto_start: bool,
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

    /// Create example config template file
    ///
    /// # Arguments
    /// * `path` - Path where to create config.example.toml
    ///
    /// # Returns
    /// * `Ok(())` - Template created successfully
    /// * `Err` - Failed to write file
    #[allow(dead_code)]
    pub fn create_template<P: AsRef<Path>>(path: P) -> Result<()> {
        let path = path.as_ref();
        let template = r#"[oracle]
endpoint = "https://NAMESPACE.compat.objectstorage.REGION.oraclecloud.com"
bucket = "my-bucket"
region = "eu-frankfurt-1"

[credentials]
# Generate with: drop2s3.exe --encrypt
version = 2
data = "BASE64_ENCRYPTED_CREDENTIALS_HERE"

[app]
auto_copy_link = true
auto_start = false

[advanced]
parallel_uploads = 3
multipart_threshold_mb = 5
multipart_chunk_mb = 5
"#;

        fs::write(path, template)
            .with_context(|| format!("Failed to write config template: {}", path.display()))?;

        Ok(())
    }

    pub fn save<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let path = path.as_ref();
        let content = toml::to_string_pretty(self)
            .context("Failed to serialize config to TOML")?;
        
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
access_key = "test_access_key"
secret_key = "test_secret_key"
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
        assert_eq!(config.oracle.access_key, "test_access_key");
        assert_eq!(config.oracle.secret_key, "test_secret_key");
        assert_eq!(config.oracle.region, "eu-frankfurt-1");
        assert_eq!(config.app.auto_copy_link, true);
        assert_eq!(config.app.auto_start, false);
        assert_eq!(config.advanced.parallel_uploads, 3);
        assert_eq!(config.advanced.multipart_threshold_mb, 5);
        assert_eq!(config.advanced.multipart_chunk_mb, 5);
    }

    #[test]
    fn test_missing_config_creates_template() {
        let temp_dir = tempfile::tempdir().unwrap();
        let template_path = temp_dir.path().join("config.example.toml");

        Config::create_template(&template_path).unwrap();

        assert!(template_path.exists());

        let content = fs::read_to_string(&template_path).unwrap();
        assert!(content.contains("[oracle]"));
        assert!(content.contains("[app]"));
        assert!(content.contains("[advanced]"));
        assert!(content.contains("endpoint ="));
        assert!(content.contains("bucket ="));
    }

    #[test]
    fn test_validate_required_fields() {
        // Test empty endpoint
        let config_content = r#"
[oracle]
endpoint = ""
bucket = "test-bucket"
access_key = "test_access_key"
secret_key = "test_secret_key"
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
access_key = "test_access_key"
secret_key = "test_secret_key"
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
        assert!(result.unwrap_err().to_string().contains("Failed to parse TOML"));
    }
}
