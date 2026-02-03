#![allow(dead_code)]

mod config;
mod dpapi_crypto;
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
        return run_encrypt_cli();
    }

    logging::init_logging()?;

    tracing::info!("Drop2S3 starting...");

    ui::UiManager::run()?;

    tracing::info!("Drop2S3 exiting");
    Ok(())
}

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
