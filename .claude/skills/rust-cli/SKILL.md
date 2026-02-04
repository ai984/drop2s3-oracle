---
name: rust-cli
description: Building professional CLI applications in Rust with clap for argument parsing, proper error handling, colored output, and configuration management. Use when creating command-line tools, parsing arguments, or handling CLI-specific concerns. Triggers on CLI, clap, command-line, argument parsing, or terminal application questions.
---

# Rust CLI Application Best Practices

You are an expert in building professional command-line applications in Rust.

## Project Setup

```toml
[package]
name = "mycli"
version = "0.1.0"
edition = "2021"

[dependencies]
# CLI argument parsing
clap = { version = "4", features = ["derive", "env", "wrap_help"] }

# Error handling
anyhow = "1"
thiserror = "1"

# Configuration
serde = { version = "1", features = ["derive"] }
toml = "0.8"
directories = "5"  # Platform-specific directories

# Output formatting
colored = "2"
indicatif = "0.17"  # Progress bars

# Logging
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
```

## Argument Parsing with Clap

### Basic Structure

```rust
use clap::{Parser, Subcommand, Args, ValueEnum};

/// A professional CLI tool for processing data
#[derive(Parser, Debug)]
#[command(name = "mycli")]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
struct Cli {
    /// Enable verbose output
    #[arg(short, long, global = true)]
    verbose: bool,

    /// Configuration file path
    #[arg(short, long, env = "MYCLI_CONFIG")]
    config: Option<PathBuf>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Initialize a new project
    Init(InitArgs),
    
    /// Process files
    Process(ProcessArgs),
    
    /// Show configuration
    Config {
        /// Show all settings
        #[arg(long)]
        all: bool,
    },
}

#[derive(Args, Debug)]
struct InitArgs {
    /// Project name
    #[arg(short, long)]
    name: String,

    /// Project template
    #[arg(short, long, value_enum, default_value_t = Template::Default)]
    template: Template,

    /// Force overwrite existing files
    #[arg(short, long)]
    force: bool,
}

#[derive(Args, Debug)]
struct ProcessArgs {
    /// Input files
    #[arg(required = true)]
    files: Vec<PathBuf>,

    /// Output directory
    #[arg(short, long, default_value = "output")]
    output: PathBuf,

    /// Output format
    #[arg(short = 'F', long, value_enum)]
    format: Option<OutputFormat>,

    /// Number of parallel workers
    #[arg(short = 'j', long, default_value_t = num_cpus::get())]
    jobs: usize,
}

#[derive(ValueEnum, Clone, Debug, Default)]
enum Template {
    #[default]
    Default,
    Minimal,
    Full,
}

#[derive(ValueEnum, Clone, Debug)]
enum OutputFormat {
    Json,
    Yaml,
    Toml,
}
```

### Main Function

```rust
use anyhow::{Context, Result};

fn main() -> Result<()> {
    // Parse arguments first (fast fail on bad args)
    let cli = Cli::parse();

    // Setup logging based on verbosity
    setup_logging(cli.verbose)?;

    // Load configuration
    let config = load_config(cli.config.as_deref())?;

    // Execute command
    match cli.command {
        Commands::Init(args) => cmd_init(args, &config),
        Commands::Process(args) => cmd_process(args, &config),
        Commands::Config { all } => cmd_config(all, &config),
    }
}

fn setup_logging(verbose: bool) -> Result<()> {
    let filter = if verbose { "debug" } else { "info" };
    
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .init();
    
    Ok(())
}
```

## Error Handling

### Custom Error Types

```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum CliError {
    #[error("Configuration error: {0}")]
    Config(#[from] ConfigError),

    #[error("File not found: {path}")]
    FileNotFound { path: PathBuf },

    #[error("Invalid input: {reason}")]
    InvalidInput { reason: String },

    #[error("Operation cancelled by user")]
    Cancelled,
}

#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("Failed to read config file: {0}")]
    ReadError(#[from] std::io::Error),

    #[error("Failed to parse config: {0}")]
    ParseError(#[from] toml::de::Error),

    #[error("Missing required field: {0}")]
    MissingField(String),
}
```

### Using anyhow for Application Errors

```rust
use anyhow::{bail, ensure, Context, Result};

fn process_file(path: &Path) -> Result<ProcessedData> {
    // Add context to errors
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read file: {}", path.display()))?;

    // Quick validation
    ensure!(!content.is_empty(), "File is empty: {}", path.display());

    // Early return with error
    if !is_valid_format(&content) {
        bail!("Invalid file format: {}", path.display());
    }

    parse_content(&content)
        .with_context(|| format!("Failed to parse: {}", path.display()))
}
```

### Exit Codes

```rust
use std::process::ExitCode;

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("Error: {e:?}");
            ExitCode::from(1)
        }
    }
}

// Or with custom exit codes
#[repr(u8)]
enum Exit {
    Success = 0,
    GeneralError = 1,
    ConfigError = 2,
    IoError = 3,
}

impl From<Exit> for ExitCode {
    fn from(code: Exit) -> Self {
        ExitCode::from(code as u8)
    }
}
```

## Configuration Management

### Config File Structure

```rust
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct Config {
    #[serde(default)]
    pub general: GeneralConfig,
    
    #[serde(default)]
    pub output: OutputConfig,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GeneralConfig {
    #[serde(default = "default_parallel")]
    pub parallel_jobs: usize,
    
    #[serde(default)]
    pub verbose: bool,
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            parallel_jobs: num_cpus::get(),
            verbose: false,
        }
    }
}

fn default_parallel() -> usize {
    num_cpus::get()
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct OutputConfig {
    pub directory: Option<PathBuf>,
    pub format: Option<String>,
}
```

### Loading Configuration

```rust
use directories::ProjectDirs;

fn load_config(custom_path: Option<&Path>) -> Result<Config> {
    // Priority: CLI arg > env var > default locations
    let config_path = custom_path
        .map(PathBuf::from)
        .or_else(|| std::env::var("MYCLI_CONFIG").ok().map(PathBuf::from))
        .or_else(default_config_path);

    match config_path {
        Some(path) if path.exists() => {
            let content = std::fs::read_to_string(&path)
                .with_context(|| format!("Failed to read config: {}", path.display()))?;
            
            toml::from_str(&content)
                .with_context(|| format!("Failed to parse config: {}", path.display()))
        }
        _ => Ok(Config::default()),
    }
}

fn default_config_path() -> Option<PathBuf> {
    ProjectDirs::from("com", "mycompany", "mycli")
        .map(|dirs| dirs.config_dir().join("config.toml"))
}
```

## Output Formatting

### Colored Output

```rust
use colored::*;

fn print_success(msg: &str) {
    println!("{} {}", "✓".green().bold(), msg);
}

fn print_warning(msg: &str) {
    eprintln!("{} {}", "⚠".yellow().bold(), msg.yellow());
}

fn print_error(msg: &str) {
    eprintln!("{} {}", "✗".red().bold(), msg.red());
}

fn print_info(msg: &str) {
    println!("{} {}", "ℹ".blue().bold(), msg);
}

// Respect NO_COLOR environment variable
fn setup_colors() {
    if std::env::var("NO_COLOR").is_ok() {
        colored::control::set_override(false);
    }
}
```

### Progress Bars

```rust
use indicatif::{ProgressBar, ProgressStyle, MultiProgress};

fn process_files_with_progress(files: &[PathBuf]) -> Result<()> {
    let pb = ProgressBar::new(files.len() as u64);
    pb.set_style(
        ProgressStyle::with_template(
            "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({eta})"
        )?
        .progress_chars("#>-")
    );

    for file in files {
        pb.set_message(file.file_name().unwrap().to_string_lossy().to_string());
        process_file(file)?;
        pb.inc(1);
    }

    pb.finish_with_message("Done!");
    Ok(())
}

// Multiple progress bars
fn parallel_progress() -> Result<()> {
    let multi = MultiProgress::new();
    
    let pb1 = multi.add(ProgressBar::new(100));
    let pb2 = multi.add(ProgressBar::new(100));
    
    // Use pb1 and pb2 from different threads
    Ok(())
}
```

### Spinners for Long Operations

```rust
use indicatif::ProgressBar;
use std::time::Duration;

fn with_spinner<T, F: FnOnce() -> Result<T>>(msg: &str, f: F) -> Result<T> {
    let spinner = ProgressBar::new_spinner();
    spinner.set_message(msg.to_string());
    spinner.enable_steady_tick(Duration::from_millis(100));
    
    let result = f();
    
    match &result {
        Ok(_) => spinner.finish_with_message(format!("{} ✓", msg)),
        Err(_) => spinner.finish_with_message(format!("{} ✗", msg)),
    }
    
    result
}

// Usage
let data = with_spinner("Loading data...", || {
    load_large_file(path)
})?;
```

## Interactive Prompts

```toml
[dependencies]
dialoguer = "0.11"
```

```rust
use dialoguer::{Confirm, Input, Select, MultiSelect, Password};

fn interactive_setup() -> Result<Config> {
    let name: String = Input::new()
        .with_prompt("Project name")
        .default("my-project".to_string())
        .interact_text()?;

    let template = Select::new()
        .with_prompt("Choose a template")
        .items(&["Default", "Minimal", "Full"])
        .default(0)
        .interact()?;

    let features: Vec<usize> = MultiSelect::new()
        .with_prompt("Select features")
        .items(&["Logging", "Config file", "Auto-update"])
        .interact()?;

    if Confirm::new()
        .with_prompt("Proceed with setup?")
        .default(true)
        .interact()?
    {
        // Continue...
    }

    let password: String = Password::new()
        .with_prompt("Enter API key")
        .interact()?;

    Ok(Config { name, template, features })
}
```

## Stdin/Stdout Handling

```rust
use std::io::{self, BufRead, Write};

fn read_from_stdin_or_file(path: Option<&Path>) -> Result<String> {
    match path {
        Some(p) if p.to_str() != Some("-") => {
            std::fs::read_to_string(p)
                .with_context(|| format!("Failed to read: {}", p.display()))
        }
        _ => {
            let stdin = io::stdin();
            let mut content = String::new();
            for line in stdin.lock().lines() {
                content.push_str(&line?);
                content.push('\n');
            }
            Ok(content)
        }
    }
}

fn write_output(data: &str, path: Option<&Path>) -> Result<()> {
    match path {
        Some(p) if p.to_str() != Some("-") => {
            std::fs::write(p, data)
                .with_context(|| format!("Failed to write: {}", p.display()))
        }
        _ => {
            io::stdout().write_all(data.as_bytes())?;
            Ok(())
        }
    }
}
```

## Shell Completions

```rust
use clap::CommandFactory;
use clap_complete::{generate, Shell};

/// Generate shell completions
#[derive(Parser)]
struct CompletionsArgs {
    /// Shell to generate completions for
    #[arg(value_enum)]
    shell: Shell,
}

fn generate_completions(shell: Shell) {
    let mut cmd = Cli::command();
    generate(shell, &mut cmd, "mycli", &mut io::stdout());
}

// In main, add a completions subcommand
// Usage: mycli completions bash > ~/.local/share/bash-completion/completions/mycli
```

## Testing CLI Applications

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use assert_cmd::Command;
    use predicates::prelude::*;

    #[test]
    fn test_help() {
        Command::cargo_bin("mycli")
            .unwrap()
            .arg("--help")
            .assert()
            .success()
            .stdout(predicate::str::contains("Usage:"));
    }

    #[test]
    fn test_version() {
        Command::cargo_bin("mycli")
            .unwrap()
            .arg("--version")
            .assert()
            .success();
    }

    #[test]
    fn test_missing_required_arg() {
        Command::cargo_bin("mycli")
            .unwrap()
            .arg("process")  // Missing required 'files' argument
            .assert()
            .failure()
            .stderr(predicate::str::contains("required"));
    }

    #[test]
    fn test_process_command() {
        let temp = tempfile::tempdir().unwrap();
        let input = temp.path().join("input.txt");
        std::fs::write(&input, "test content").unwrap();

        Command::cargo_bin("mycli")
            .unwrap()
            .args(["process", input.to_str().unwrap()])
            .assert()
            .success();
    }
}
```

## References

- [Clap Documentation](https://docs.rs/clap)
- [Command Line Applications in Rust](https://rust-cli.github.io/book/)
- [indicatif Examples](https://github.com/console-rs/indicatif)
- [dialoguer Examples](https://github.com/console-rs/dialoguer)
