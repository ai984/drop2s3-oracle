#![allow(dead_code)]
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod config;
mod dpapi_crypto;
mod embedded_icons;
mod history;
mod logging;
mod portable_crypto;
mod startup;
mod tray;
mod ui;
mod update;
mod upload;
mod utils;

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    
    if args.iter().any(|a| a == "--encrypt") {
        attach_console();
        return run_encrypt_cli();
    }

    logging::init_logging()?;

    // Apply pending update if exists (from previous download)
    if let Err(e) = update::UpdateManager::apply_update_on_restart() {
        tracing::warn!("Failed to apply update: {}", e);
    }

    tracing::info!("Drop2S3 starting...");

    ui::UiManager::run()?;

    tracing::info!("Drop2S3 exiting");
    Ok(())
}

#[cfg(windows)]
fn attach_console() {
    use windows::Win32::System::Console::{AttachConsole, ATTACH_PARENT_PROCESS};
    unsafe { let _ = AttachConsole(ATTACH_PARENT_PROCESS); }
}

#[cfg(not(windows))]
fn attach_console() {}

fn run_encrypt_cli() -> anyhow::Result<()> {
    use std::io::{self, Write};
    
    println!("Drop2S3 Credential Encryption Tool");
    println!("===================================");
    println!();
    
    print!("Access Key: ");
    io::stdout().flush()?;
    let mut access_key = String::new();
    io::stdin().read_line(&mut access_key)?;
    let access_key = access_key.trim();
    
    if access_key.is_empty() {
        anyhow::bail!("Access key cannot be empty");
    }
    
    print!("Secret Key: ");
    io::stdout().flush()?;
    let mut secret_key = String::new();
    io::stdin().read_line(&mut secret_key)?;
    let secret_key = secret_key.trim();
    
    if secret_key.is_empty() {
        anyhow::bail!("Secret key cannot be empty");
    }
    
    let encrypted = portable_crypto::encrypt_credentials(access_key, secret_key)?;
    
    println!();
    println!("Add this to your config.toml:");
    println!("------------------------------");
    println!("[credentials]");
    println!("version = {}", encrypted.version);
    println!("data = \"{}\"", encrypted.data);
    println!();
    
    Ok(())
}
