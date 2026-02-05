use anyhow::Result;
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

/// Initialize tracing with file appender, daily rotation, and structured logging.
///
/// Creates logs directory next to the executable with daily log files.
/// Log format includes timestamps, levels, and targets.
pub fn init_logging() -> Result<()> {
    let logs_dir = crate::utils::get_exe_dir().join("logs");
    std::fs::create_dir_all(&logs_dir)?;

    let file_appender = RollingFileAppender::new(Rotation::DAILY, &logs_dir, "drop2s3.log");

    let fmt_layer = fmt::layer()
        .with_writer(file_appender)
        .with_ansi(false)
        .with_target(true)
        .with_level(true)
        .with_thread_ids(false)
        .with_thread_names(false)
        .with_timer(fmt::time::SystemTime);

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
    #[test]
    fn test_log_directory_path_construction() {
        let exe_dir = crate::utils::get_exe_dir();
        let logs_dir = exe_dir.join("logs");
        
        assert!(logs_dir.ends_with("logs"));
        assert!(logs_dir.parent().is_some());
    }
}
