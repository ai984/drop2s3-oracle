mod config;
mod crypto;
mod history;
mod logging;
mod startup;
mod tray;
mod ui;
mod update;
mod upload;
mod utils;

use tray::{MenuAction, TrayManager};

fn main() -> anyhow::Result<()> {
    logging::init_logging()?;

    tracing::info!("Drop2S3 starting...");

    let tray_manager = TrayManager::new()?;

    tracing::info!("Tray icon created, entering event loop");

    loop {
        if let Some(event) = TrayManager::poll_tray_event() {
            tray_manager.handle_tray_event(&event);
        }

        if let Some(event) = TrayManager::poll_menu_event() {
            let action = tray_manager.handle_menu_event(&event);

            match action {
                MenuAction::Quit => {
                    tracing::info!("Quit requested, exiting...");
                    break;
                }
                MenuAction::ShowWindow => {
                    tracing::info!("Show window requested (not implemented yet)");
                }
                MenuAction::ShowSettings => {
                    tracing::info!("Show settings requested (not implemented yet)");
                }
                MenuAction::None => {}
            }
        }

        std::thread::sleep(std::time::Duration::from_millis(100));
    }

    tracing::info!("Drop2S3 exiting");
    Ok(())
}
