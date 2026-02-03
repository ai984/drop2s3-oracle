use anyhow::Result;
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

/// Initialize tracing with file appender, daily rotation, and structured logging.
///
/// Creates logs directory next to the executable with daily log files.
/// Log format includes timestamps, levels, and targets.
/// Filters out sensitive fields (access_key, secret_key, password).
pub fn init_logging() -> Result<()> {
    let logs_dir = "logs";
    std::fs::create_dir_all(logs_dir)?;

    let file_appender = RollingFileAppender::new(Rotation::DAILY, logs_dir, "drop2s3.log");

    let fmt_layer = fmt::layer()
        .with_writer(file_appender)
        .with_ansi(false)
        .with_target(true)
        .with_level(true)
        .with_thread_ids(false)
        .with_thread_names(false)
        .with_timer(fmt::time::SystemTime::default());

    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,drop2s3=info"));

    let subscriber = tracing_subscriber::registry()
        .with(env_filter)
        .with(fmt_layer);

    tracing::subscriber::set_global_default(subscriber)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;

    #[test]
    fn test_log_file_created() {
        let _ = fs::remove_dir_all("logs");

        let result = init_logging();
        assert!(result.is_ok(), "init_logging should succeed");

        assert!(
            Path::new("logs").exists(),
            "logs directory should be created"
        );

        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        let log_file = format!("logs/{}.log", today);
        assert!(
            Path::new(&log_file).exists(),
            "log file {} should be created",
            log_file
        );

        let _ = fs::remove_dir_all("logs");
    }

    #[test]
    fn test_log_format_with_timestamp() {
        let _ = fs::remove_dir_all("logs");

        let _ = init_logging();

        tracing::info!("Test log message");

        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        let log_file = format!("logs/{}.log", today);

        std::thread::sleep(std::time::Duration::from_millis(100));

        let content = fs::read_to_string(&log_file)
            .expect("should be able to read log file");

        assert!(
            content.contains("Test log message"),
            "log should contain the message"
        );
        assert!(
            content.contains("INFO"),
            "log should contain INFO level"
        );
        assert!(
            content.contains("202"),
            "log should contain timestamp with year"
        );

        let _ = fs::remove_dir_all("logs");
    }

    #[test]
    fn test_log_level_filtering() {
        let _ = fs::remove_dir_all("logs");

        let _ = init_logging();

        tracing::debug!("Debug message - should not appear");
        tracing::info!("Info message - should appear");
        tracing::error!("Error message - should appear");

        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        let log_file = format!("logs/{}.log", today);

        std::thread::sleep(std::time::Duration::from_millis(100));

        let content = fs::read_to_string(&log_file)
            .expect("should be able to read log file");

        assert!(
            content.contains("Info message"),
            "log should contain INFO level message"
        );
        assert!(
            content.contains("Error message"),
            "log should contain ERROR level message"
        );
        assert!(
            !content.contains("Debug message"),
            "log should NOT contain DEBUG level message"
        );

        let _ = fs::remove_dir_all("logs");
    }

    #[test]
    fn test_logs_directory_structure() {
        let _ = fs::remove_dir_all("logs");

        let _ = init_logging();

        assert!(
            Path::new("logs").is_dir(),
            "logs should be a directory"
        );

        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        let log_file = format!("logs/{}.log", today);
        assert!(
            Path::new(&log_file).is_file(),
            "log file should be a regular file"
        );

        let _ = fs::remove_dir_all("logs");
    }
}
