use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use anyhow::Result;

const MAX_ENTRIES: usize = 10;
const MAX_FILE_SIZE: u64 = 1_048_576;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct HistoryEntry {
    pub filename: String,
    pub url: String,
    pub timestamp: DateTime<Utc>,
    pub size: u64,
}

pub struct History {
    inner: Mutex<HistoryInner>,
}

struct HistoryInner {
    entries: Vec<HistoryEntry>,
    file_path: PathBuf,
}

impl History {
    pub fn new(file_path: impl AsRef<Path>) -> Result<Self> {
        let file_path = file_path.as_ref().to_path_buf();
        let mut inner = HistoryInner {
            entries: Vec::new(),
            file_path,
        };
        inner.load_from_disk()?;
        Ok(History {
            inner: Mutex::new(inner),
        })
    }

    pub fn add(&self, filename: &str, url: &str) {
        let entry = HistoryEntry {
            filename: filename.to_string(),
            url: url.to_string(),
            timestamp: Utc::now(),
            size: 0,
        };
        
        if let Ok(mut inner) = self.inner.lock() {
            inner.entries.insert(0, entry);
            if inner.entries.len() > MAX_ENTRIES {
                inner.entries.truncate(MAX_ENTRIES);
            }
            let _ = inner.save_to_disk();
        }
    }

    pub fn get_all(&self) -> Vec<HistoryEntry> {
        self.inner
            .lock()
            .map(|inner| inner.entries.clone())
            .unwrap_or_default()
    }

    #[allow(dead_code)]
    pub fn clear(&self) -> Result<()> {
        let mut inner = self.inner.lock().map_err(|_| anyhow::anyhow!("Lock error"))?;
        inner.entries.clear();
        inner.save_to_disk()
    }
}

impl HistoryInner {
    fn load_from_disk(&mut self) -> Result<()> {
        if !self.file_path.exists() {
            return Ok(());
        }

        let metadata = fs::metadata(&self.file_path)?;
        if metadata.len() > MAX_FILE_SIZE {
            self.entries.clear();
            self.save_to_disk()?;
            return Ok(());
        }

        let content = fs::read_to_string(&self.file_path)?;
        if content.is_empty() {
            return Ok(());
        }

        match serde_json::from_str::<Vec<HistoryEntry>>(&content) {
            Ok(entries) => {
                self.entries = entries;
                Ok(())
            }
            Err(_) => {
                self.entries.clear();
                Ok(())
            }
        }
    }

    fn save_to_disk(&self) -> Result<()> {
        let json = serde_json::to_string_pretty(&self.entries)?;
        fs::write(&self.file_path, json)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_add_entry() {
        let temp_dir = TempDir::new().unwrap();
        let history_path = temp_dir.path().join("history.json");

        let history = History::new(&history_path).unwrap();
        history.add("test.txt", "https://example.com/test.txt");

        let entries = history.get_all();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].filename, "test.txt");
    }

    #[test]
    fn test_fifo_at_limit() {
        let temp_dir = TempDir::new().unwrap();
        let history_path = temp_dir.path().join("history.json");

        let history = History::new(&history_path).unwrap();

        for i in 0..12 {
            history.add(&format!("file{}.txt", i), &format!("https://example.com/file{}.txt", i));
        }

        let entries = history.get_all();
        assert_eq!(entries.len(), MAX_ENTRIES);
        assert_eq!(entries[0].filename, "file11.txt");
    }

    #[test]
    fn test_persistence() {
        let temp_dir = TempDir::new().unwrap();
        let history_path = temp_dir.path().join("history.json");

        {
            let history = History::new(&history_path).unwrap();
            history.add("file.txt", "https://example.com/file.txt");
        }

        let history = History::new(&history_path).unwrap();
        let entries = history.get_all();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].filename, "file.txt");
    }
}
