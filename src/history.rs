use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use anyhow::Result;

#[allow(dead_code)]
const MAX_ENTRIES: usize = 5;
const MAX_FILE_SIZE: u64 = 1_048_576; // 1 MB

/// Represents a single history entry for an uploaded file
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct HistoryEntry {
    pub filename: String,
    pub url: String,
    pub timestamp: DateTime<Utc>,
    pub size: u64,
}

/// Manages history storage with FIFO eviction and file size limits
pub struct History {
    entries: Vec<HistoryEntry>,
    file_path: PathBuf,
}

impl History {
    /// Creates a new History manager and loads existing entries from disk
    pub fn new(file_path: impl AsRef<Path>) -> Result<Self> {
        let file_path = file_path.as_ref().to_path_buf();
        let mut history = History {
            entries: Vec::new(),
            file_path,
        };
        history.load_from_disk()?;
        Ok(history)
    }

    #[allow(dead_code)]
    pub fn add(&mut self, entry: HistoryEntry) -> Result<()> {
        // Add to front (most recent first)
        self.entries.insert(0, entry);

        // Enforce max entries limit (FIFO: remove oldest)
        if self.entries.len() > MAX_ENTRIES {
            self.entries.truncate(MAX_ENTRIES);
        }

        // Save to disk
        self.save_to_disk()?;
        Ok(())
    }

    /// Returns all history entries in order (most recent first)
    pub fn get_all(&self) -> Vec<HistoryEntry> {
        self.entries.clone()
    }

    #[allow(dead_code)]
    pub fn clear(&mut self) -> Result<()> {
        self.entries.clear();
        self.save_to_disk()?;
        Ok(())
    }

    /// Loads history from disk, handling file size limits
    fn load_from_disk(&mut self) -> Result<()> {
        if !self.file_path.exists() {
            return Ok(());
        }

        // Check file size and truncate if necessary
        let metadata = fs::metadata(&self.file_path)?;
        if metadata.len() > MAX_FILE_SIZE {
            // File is too large, truncate to last 5 entries
            self.entries.clear();
            self.save_to_disk()?;
            return Ok(());
        }

        // Read and parse JSON
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
                // Corrupted file, start fresh
                self.entries.clear();
                Ok(())
            }
        }
    }

    /// Saves history to disk as JSON
    fn save_to_disk(&self) -> Result<()> {
        let json = serde_json::to_string_pretty(&self.entries)?;
        fs::write(&self.file_path, json)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn create_test_entry(filename: &str, url: &str, size: u64) -> HistoryEntry {
        HistoryEntry {
            filename: filename.to_string(),
            url: url.to_string(),
            timestamp: Utc::now(),
            size,
        }
    }

    #[test]
    fn test_add_entry() {
        let temp_dir = TempDir::new().unwrap();
        let history_path = temp_dir.path().join("history.json");

        let mut history = History::new(&history_path).unwrap();
        let entry = create_test_entry("test.txt", "https://example.com/test.txt", 1024);

        history.add(entry.clone()).unwrap();

        let entries = history.get_all();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].filename, "test.txt");
        assert_eq!(entries[0].url, "https://example.com/test.txt");
        assert_eq!(entries[0].size, 1024);
    }

    #[test]
    fn test_fifo_at_limit() {
        let temp_dir = TempDir::new().unwrap();
        let history_path = temp_dir.path().join("history.json");

        let mut history = History::new(&history_path).unwrap();

        // Add 6 entries
        for i in 0..6 {
            let entry = create_test_entry(
                &format!("file{}.txt", i),
                &format!("https://example.com/file{}.txt", i),
                1024 * (i as u64 + 1),
            );
            history.add(entry).unwrap();
        }

        // Should only have 5 entries
        let entries = history.get_all();
        assert_eq!(entries.len(), 5);

        // Most recent should be file5, oldest should be file1 (file0 removed)
        assert_eq!(entries[0].filename, "file5.txt");
        assert_eq!(entries[4].filename, "file1.txt");
    }

    #[test]
    fn test_persistence() {
        let temp_dir = TempDir::new().unwrap();
        let history_path = temp_dir.path().join("history.json");

        // Create and populate history
        {
            let mut history = History::new(&history_path).unwrap();
            for i in 0..3 {
                let entry = create_test_entry(
                    &format!("file{}.txt", i),
                    &format!("https://example.com/file{}.txt", i),
                    1024 * (i as u64 + 1),
                );
                history.add(entry).unwrap();
            }
        }

        // Reload and verify
        let history = History::new(&history_path).unwrap();
        let entries = history.get_all();

        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].filename, "file2.txt");
        assert_eq!(entries[1].filename, "file1.txt");
        assert_eq!(entries[2].filename, "file0.txt");
    }

    #[test]
    fn test_clear() {
        let temp_dir = TempDir::new().unwrap();
        let history_path = temp_dir.path().join("history.json");

        let mut history = History::new(&history_path).unwrap();
        let entry = create_test_entry("test.txt", "https://example.com/test.txt", 1024);
        history.add(entry).unwrap();

        assert_eq!(history.get_all().len(), 1);

        history.clear().unwrap();
        assert_eq!(history.get_all().len(), 0);

        // Verify file is empty
        let reloaded = History::new(&history_path).unwrap();
        assert_eq!(reloaded.get_all().len(), 0);
    }

    #[test]
    fn test_file_size_limit() {
        let temp_dir = TempDir::new().unwrap();
        let history_path = temp_dir.path().join("history.json");

        // Create a file that exceeds size limit
        let oversized_json = r#"[
            {"filename":"test1.txt","url":"https://example.com/test1.txt","timestamp":"2026-02-03T12:00:00Z","size":1024},
            {"filename":"test2.txt","url":"https://example.com/test2.txt","timestamp":"2026-02-03T12:00:00Z","size":2048}
        ]"#;

        // Pad with extra data to exceed 1MB
        let padded = format!("{}\n{}", oversized_json, "x".repeat(1_048_600));
        fs::write(&history_path, padded).unwrap();

        // Load should handle oversized file gracefully
        let history = History::new(&history_path).unwrap();
        assert_eq!(history.get_all().len(), 0);
    }
}
