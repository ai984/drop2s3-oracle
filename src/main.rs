#![allow(dead_code)]
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod config;
mod dpapi_crypto;
mod embedded_icons;
mod history;
mod logging;
mod portable_crypto;
mod single_instance;
mod startup;
mod tray;
mod ui;
mod update;
mod upload;
mod utils;

use anyhow::{Context, Result};
use std::sync::Arc;
use std::time::Duration;

use history::History;
use tray::{MenuAction, TrayManager};
use upload::{S3Client, UploadManager, UploadProgress};

pub struct AppState {
    pub rt_handle: tokio::runtime::Handle,
    pub upload_manager: Arc<UploadManager>,
    pub progress_rx: std::sync::Mutex<tokio::sync::mpsc::UnboundedReceiver<UploadProgress>>,
    pub history: Arc<History>,
    pub tray_manager: std::sync::Mutex<TrayManager>,
    pub update_state: Arc<std::sync::Mutex<ui::UpdateState>>,
    pub config: std::sync::Mutex<config::Config>,
    pub config_path: std::path::PathBuf,
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();

    if args.iter().any(|a| a == "--encrypt") {
        attach_console();
        return run_encrypt_cli();
    }

    if args.iter().any(|a| a == "--init-robots") {
        attach_console();
        return run_init_robots_cli();
    }

    let _instance_guard = match single_instance::SingleInstanceGuard::acquire() {
        Ok(guard) => guard,
        Err(_) => {
            single_instance::show_already_running_message();
            return Ok(());
        }
    };

    logging::init_logging()?;
    update::UpdateManager::cleanup_old_version();
    tracing::info!("Drop2S3 v{} starting...", env!("CARGO_PKG_VERSION"));

    let (rt, app_state) = initialize_app_state()?;
    #[allow(clippy::arc_with_non_send_sync)]
    let app_state = Arc::new(app_state);
    start_update_check(&app_state);
    run_main_loop(rt, app_state)?;

    tracing::info!("Drop2S3 exiting");
    Ok(())
}

fn initialize_app_state() -> Result<(tokio::runtime::Runtime, AppState)> {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .context("Failed to create tokio runtime")?;

    let rt_handle = rt.handle().clone();

    let config_path = utils::get_exe_dir().join("config.toml");
    let config = config::Config::load(&config_path).context("Failed to load config")?;

    let s3_client = rt
        .block_on(S3Client::new(&config))
        .context("Failed to create S3 client")?;

    let (upload_manager, progress_rx) =
        UploadManager::new(s3_client, config.advanced.parallel_uploads as usize, 3);

    let history_path = utils::get_exe_dir().join("history.json");
    let history = History::new(&history_path).context("Failed to load history")?;

    let tray_manager = TrayManager::new().context("Failed to create system tray")?;

    let app_state = AppState {
        rt_handle,
        upload_manager: Arc::new(upload_manager),
        progress_rx: std::sync::Mutex::new(progress_rx),
        history: Arc::new(history),
        tray_manager: std::sync::Mutex::new(tray_manager),
        update_state: Arc::new(std::sync::Mutex::new(ui::UpdateState::Checking)),
        config: std::sync::Mutex::new(config),
        config_path,
    };

    Ok((rt, app_state))
}

fn start_update_check(app_state: &Arc<AppState>) {
    let update_state = app_state.update_state.clone();
    app_state.rt_handle.spawn(async move {
        let manager = update::UpdateManager::new();
        match manager.check_for_updates().await {
            Ok(Some(version)) => {
                if let Ok(mut state) = update_state.lock() {
                    *state = ui::UpdateState::Downloading;
                }
                match manager.download_update(&version).await {
                    Ok(()) => {
                        if let Ok(mut state) = update_state.lock() {
                            *state = ui::UpdateState::ReadyToInstall;
                        }
                    }
                    Err(_) => {
                        if let Ok(mut state) = update_state.lock() {
                            *state = ui::UpdateState::None;
                        }
                    }
                }
            }
            Ok(None) | Err(_) => {
                if let Ok(mut state) = update_state.lock() {
                    *state = ui::UpdateState::None;
                }
            }
        }
    });
}

fn run_main_loop(rt: tokio::runtime::Runtime, app_state: Arc<AppState>) -> Result<()> {
    tracing::info!("Entering main loop (lightweight mode)");

    let mut should_show_window = true;
    let mut loop_count = 0u64;

    loop {
        pump_windows_messages();
        
        loop_count += 1;
        if loop_count.is_multiple_of(100) {
            tracing::debug!("Main loop iteration {}", loop_count);
        }

        if TrayManager::quit_requested() {
            tracing::info!("Quit requested, shutting down...");
            if let Err(e) = update::UpdateManager::apply_update_on_shutdown() {
                tracing::warn!("Failed to apply update: {}", e);
            }
            break;
        }

        if TrayManager::should_show_window() {
            tracing::info!("Tray: show window requested");
            should_show_window = true;
        }

        while let Some(event) = TrayManager::poll_menu_event() {
            tracing::info!("Tray: menu event received: {:?}", event.id);
            if let Ok(tray) = app_state.tray_manager.lock() {
                match tray.handle_menu_event(&event) {
                    MenuAction::ShowWindow => should_show_window = true,
                    MenuAction::Quit => {
                        tracing::info!("Quit from menu");
                        if let Err(e) = update::UpdateManager::apply_update_on_shutdown() {
                            tracing::warn!("Failed to apply update: {}", e);
                        }
                        rt.shutdown_timeout(Duration::from_millis(500));
                        return Ok(());
                    }
                    MenuAction::None => {}
                }
            }
        }

        if should_show_window {
            should_show_window = false;
            tracing::info!("Showing window (heavy mode)");

            if let Err(e) = ui::show_window(app_state.clone()) {
                tracing::error!("Window error: {}", e);
            }

            tracing::info!("Window closed, back to lightweight mode");

            if TrayManager::quit_requested() {
                if let Err(e) = update::UpdateManager::apply_update_on_shutdown() {
                    tracing::warn!("Failed to apply update: {}", e);
                }
                break;
            }
        }

        std::thread::sleep(Duration::from_millis(50));
    }

    rt.shutdown_timeout(Duration::from_millis(500));

    Ok(())
}

#[cfg(windows)]
fn pump_windows_messages() {
    use windows::Win32::UI::WindowsAndMessaging::{
        DispatchMessageW, PeekMessageW, TranslateMessage, MSG, PM_REMOVE,
    };

    unsafe {
        let mut msg = MSG::default();
        while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

#[cfg(not(windows))]
fn pump_windows_messages() {}

#[cfg(windows)]
fn attach_console() {
    use windows::Win32::System::Console::{AttachConsole, ATTACH_PARENT_PROCESS};
    unsafe {
        let _ = AttachConsole(ATTACH_PARENT_PROCESS);
    }
}

#[cfg(not(windows))]
fn attach_console() {}

fn run_encrypt_cli() -> Result<()> {
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

fn run_init_robots_cli() -> Result<()> {
    println!("Drop2S3 - Upload robots.txt");
    println!("===========================");
    println!();

    let config_path = utils::get_exe_dir().join("config.toml");
    let config = config::Config::load(&config_path).context("Failed to load config")?;

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("Failed to create tokio runtime")?;

    let url = rt.block_on(async {
        let client = upload::S3Client::new(&config)
            .await
            .context("Failed to create S3 client")?;

        client.upload_robots_txt().await
    })?;

    println!("robots.txt uploaded successfully!");
    println!("URL: {}", url);
    println!();
    println!("Content:");
    println!("  User-agent: *");
    println!("  Disallow: /");

    Ok(())
}
