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
    logging::init_logging()?;

    tracing::info!("Drop2S3 starting...");

    ui::UiManager::run()?;

    tracing::info!("Drop2S3 exiting");
    Ok(())
}
