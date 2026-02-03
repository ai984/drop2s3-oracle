use anyhow::{Context, Result};
use eframe::egui;
use std::path::PathBuf;
use std::sync::Arc;

use crate::config::Config;
use crate::tray::{MenuAction, TrayManager};
use crate::upload::{S3Client, UploadManager, UploadProgress};

pub struct UiManager;

impl UiManager {
    pub fn run() -> Result<()> {
        let (upload_manager, progress_rx, cancel_token) = initialize_upload_manager()?;
        let upload_manager = Arc::new(upload_manager);

        let tray_manager = TrayManager::new()
            .context("Failed to create system tray")?;

        let options = eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_inner_size([200.0, 250.0])
                .with_min_inner_size([200.0, 250.0])
                .with_always_on_top()
                .with_resizable(true)
                .with_visible(false),
            ..Default::default()
        };

        eframe::run_native(
            "Drop2S3",
            options,
            Box::new(move |_cc| {
                Box::new(DropZoneApp {
                    tray_manager,
                    upload_manager,
                    progress_rx,
                    cancel_token,
                })
            }),
        )
        .map_err(|e| anyhow::anyhow!("eframe error: {}", e))?;

        Ok(())
    }
}

fn initialize_upload_manager() -> Result<(
    UploadManager,
    tokio::sync::mpsc::UnboundedReceiver<UploadProgress>,
    tokio_util::sync::CancellationToken,
)> {
    let rt = tokio::runtime::Runtime::new().context("Failed to create tokio runtime")?;

    let config = Config::load(std::path::Path::new("config.toml"))
        .context("Failed to load config")?;

    let s3_client = rt
        .block_on(S3Client::new(&config))
        .context("Failed to create S3 client")?;

    let (upload_manager, progress_rx, cancel_token) = UploadManager::new(
        s3_client,
        config.advanced.parallel_uploads as usize,
        3,
    );

    // Runtime must stay alive for tokio::spawn to work in DropZoneApp
    Box::leak(Box::new(rt));

    Ok((upload_manager, progress_rx, cancel_token))
}

struct DropZoneApp {
    tray_manager: TrayManager,
    upload_manager: Arc<UploadManager>,
    #[allow(dead_code)]
    progress_rx: tokio::sync::mpsc::UnboundedReceiver<UploadProgress>,
    #[allow(dead_code)]
    cancel_token: tokio_util::sync::CancellationToken,
}

impl eframe::App for DropZoneApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if let Some(event) = TrayManager::poll_tray_event() {
            self.tray_manager.handle_tray_event(&event);
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        }

        if let Some(event) = TrayManager::poll_menu_event() {
            let action = self.tray_manager.handle_menu_event(&event);

            match action {
                MenuAction::Quit => {
                    tracing::info!("Quit action received, closing window");
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
                MenuAction::ShowWindow => {
                    tracing::info!("Show window action received");
                    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                    ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                }
                MenuAction::ShowSettings => {
                    tracing::info!("Show settings action received (not implemented yet)");
                }
                MenuAction::None => {}
            }
        }

        let dropped_files: Vec<PathBuf> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .filter_map(|f| f.path.as_ref())
                .filter(|p| p.is_file())
                .cloned()
                .collect()
        });

        if !dropped_files.is_empty() {
            tracing::info!("Files dropped: {} files", dropped_files.len());
            let manager = self.upload_manager.clone();
            tokio::spawn(async move {
                match manager.upload_files(dropped_files).await {
                    Ok(urls) => {
                        tracing::info!("Upload completed: {} files", urls.len());
                        for url in urls {
                            tracing::info!("  - {}", url);
                        }
                    }
                    Err(e) => {
                        tracing::error!("Upload failed: {}", e);
                    }
                }
            });
        }

        let is_hovering = ctx.input(|i| !i.raw.hovered_files.is_empty());

        egui::CentralPanel::default().show(ctx, |ui| {
            if is_hovering {
                ui.visuals_mut().widgets.noninteractive.bg_fill =
                    egui::Color32::from_rgb(100, 180, 100);
            }

            ui.vertical_centered(|ui| {
                ui.add_space(80.0);
                ui.heading("☁️");
                ui.add_space(20.0);
                if is_hovering {
                    ui.label("⬇️ Upuść tutaj");
                } else {
                    ui.label("Upuść plik");
                }
            });
        });

        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            tracing::debug!("ESC pressed, ignoring");
        }

        ctx.request_repaint();
    }
}
