use anyhow::{Context, Result};
use eframe::egui;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::history::History;
use crate::tray::{MenuAction, TrayManager};
use crate::upload::{S3Client, UploadManager, UploadProgress};

pub struct UiManager;

impl UiManager {
    pub fn run() -> Result<()> {
        let (upload_manager, progress_rx, cancel_token) = initialize_upload_manager()?;
        let upload_manager = Arc::new(upload_manager);

        let tray_manager = TrayManager::new()
            .context("Failed to create system tray")?;

        let history_path = crate::utils::get_exe_dir().join("history.json");
        let history = History::new(&history_path)
            .context("Failed to load history")?;

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
                    current_upload: None,
                    history,
                    copy_feedback: None,
                    is_uploading: false,
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

    let config_path = crate::utils::get_exe_dir().join("config.toml");
    let config = crate::config::Config::load(&config_path)
        .context("Failed to load config")?;

    let s3_client = rt
        .block_on(S3Client::new(&config))
        .context("Failed to create S3 client")?;

    let (upload_manager, progress_rx, cancel_token) = UploadManager::new(
        s3_client,
        config.advanced.parallel_uploads as usize,
        3,
    );

    Box::leak(Box::new(rt));

    Ok((upload_manager, progress_rx, cancel_token))
}

struct DropZoneApp {
    tray_manager: TrayManager,
    upload_manager: Arc<UploadManager>,
    progress_rx: tokio::sync::mpsc::UnboundedReceiver<UploadProgress>,
    cancel_token: tokio_util::sync::CancellationToken,
    current_upload: Option<UploadProgress>,
    history: History,
    copy_feedback: Option<(String, Instant)>,
    is_uploading: bool,
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

        while let Ok(progress) = self.progress_rx.try_recv() {
            use crate::upload::UploadStatus;
            
            match &progress.status {
                UploadStatus::Uploading => {
                    if !self.is_uploading {
                        self.is_uploading = true;
                        let icon_path = crate::utils::get_exe_dir().join("assets/icon_uploading.ico");
                        if let Err(e) = self.tray_manager.set_icon(icon_path.to_str().unwrap_or("assets/icon_uploading.ico")) {
                            tracing::error!("Failed to set uploading icon: {}", e);
                        }
                    }
                    self.current_upload = Some(progress);
                }
                UploadStatus::Completed | UploadStatus::Failed(_) | UploadStatus::Cancelled => {
                    if self.is_uploading {
                        self.is_uploading = false;
                        let icon_path = crate::utils::get_exe_dir().join("assets/icon.ico");
                        if let Err(e) = self.tray_manager.set_icon(icon_path.to_str().unwrap_or("assets/icon.ico")) {
                            tracing::error!("Failed to restore static icon: {}", e);
                        }
                    }
                    self.current_upload = None;
                }
                _ => {
                    self.current_upload = Some(progress);
                }
            }
        }

        if let Some(progress) = &self.current_upload {
            use crate::upload::UploadStatus;
            
            egui::Window::new("Upload Progress")
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| {
                    ui.vertical(|ui| {
                        ui.label(&progress.filename);
                        ui.add_space(5.0);
                        
                        let fraction = if progress.total_bytes > 0 {
                            progress.bytes_uploaded as f32 / progress.total_bytes as f32
                        } else {
                            0.0
                        };
                        
                        ui.add(
                            egui::ProgressBar::new(fraction)
                                .show_percentage()
                        );
                        ui.add_space(5.0);
                        
                        let bytes_uploaded_mb = progress.bytes_uploaded as f64 / (1024.0 * 1024.0);
                        let total_bytes_mb = progress.total_bytes as f64 / (1024.0 * 1024.0);
                        ui.label(format!("{:.2} MB / {:.2} MB", bytes_uploaded_mb, total_bytes_mb));
                        
                        ui.add_space(5.0);
                        
                        let status_text = match &progress.status {
                            UploadStatus::Queued => "W kolejce...",
                            UploadStatus::Uploading => "Przesyłanie...",
                            UploadStatus::Completed => "Ukończono",
                            UploadStatus::Failed(err) => &format!("Błąd: {}", err),
                            UploadStatus::Cancelled => "Anulowano",
                        };
                        ui.label(status_text);
                        
                        ui.add_space(10.0);
                        
                        if matches!(progress.status, UploadStatus::Queued | UploadStatus::Uploading)
                            && ui.button("Anuluj").clicked()
                        {
                            self.cancel_token.cancel();
                            tracing::info!("Upload cancelled by user");
                        }
                    });
                });
        }

        // History window
        egui::Window::new("Historia")
            .collapsible(true)
            .resizable(true)
            .default_width(300.0)
            .show(ctx, |ui| {
                let entries = self.history.get_all();
                
                if entries.is_empty() {
                    ui.label("Brak historii");
                } else {
                    egui::ScrollArea::vertical()
                        .auto_shrink([false; 2])
                        .show(ui, |ui| {
                            for entry in entries.iter().take(5) {
                                ui.horizontal(|ui| {
                                    // Truncate filename if too long
                                    let display_name = if entry.filename.len() > 25 {
                                        format!("{}...", &entry.filename[..22])
                                    } else {
                                        entry.filename.clone()
                                    };
                                    
                                    // Check for double-click
                                    let response = ui.label(&display_name);
                                    if response.double_clicked() {
                                        let url = entry.url.clone();
                                        if let Err(e) = open_url_in_browser(&url) {
                                            tracing::error!("Failed to open URL: {}", e);
                                        }
                                    }
                                    
                                    if ui.button("Kopiuj").clicked() {
                                        match arboard::Clipboard::new() {
                                            Ok(mut clipboard) => {
                                                match clipboard.set_text(entry.url.clone()) {
                                                    Ok(_) => {
                                                        self.copy_feedback = Some((entry.filename.clone(), Instant::now()));
                                                        tracing::info!("Copied to clipboard: {}", entry.filename);
                                                    }
                                                    Err(e) => {
                                                        tracing::error!("Failed to copy to clipboard: {}", e);
                                                    }
                                                }
                                            }
                                            Err(e) => {
                                                tracing::error!("Failed to access clipboard: {}", e);
                                            }
                                        }
                                    }
                                });
                            }
                        });
                }
            });

        // Copy feedback notification
        if let Some((filename, instant)) = &self.copy_feedback {
            let elapsed = instant.elapsed();
            if elapsed < Duration::from_secs(2) {
                egui::Window::new("Powiadomienie")
                    .collapsible(false)
                    .resizable(false)
                    .show(ctx, |ui| {
                        ui.label(format!("Skopiowano: {}", filename));
                    });
            } else {
                self.copy_feedback = None;
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

        let ctrl_v_pressed = ctx.input(|i| i.key_pressed(egui::Key::V) && i.modifiers.ctrl);
        if ctrl_v_pressed {
            tracing::info!("Ctrl+V pressed, attempting to paste image from clipboard");
            match arboard::Clipboard::new() {
                Ok(mut clipboard) => {
                    match clipboard.get_image() {
                        Ok(image_data) => {
                            let filename = generate_screenshot_filename();
                            tracing::info!("Image data retrieved from clipboard: {}", filename);
                            
                            match save_image_to_temp(&image_data, &filename) {
                                Ok(temp_path) => {
                                    tracing::info!("Image saved to temp file: {}", temp_path.display());
                                    let manager = self.upload_manager.clone();
                                    tokio::spawn(async move {
                                        match manager.upload_files(vec![temp_path.clone()]).await {
                                            Ok(urls) => {
                                                tracing::info!("Screenshot upload completed: {}", urls.join(", "));
                                                let _ = std::fs::remove_file(&temp_path);
                                            }
                                            Err(e) => {
                                                tracing::error!("Screenshot upload failed: {}", e);
                                                let _ = std::fs::remove_file(&temp_path);
                                            }
                                        }
                                    });
                                }
                                Err(e) => {
                                    tracing::error!("Failed to save image to temp file: {}", e);
                                }
                            }
                        }
                        Err(e) => {
                            tracing::warn!("Clipboard does not contain image data: {}", e);
                        }
                    }
                }
                Err(e) => {
                    tracing::error!("Failed to access clipboard: {}", e);
                }
            }
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

fn open_url_in_browser(url: &str) -> Result<()> {
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("cmd")
            .args(["/C", "start", url])
            .spawn()
            .context("Failed to open URL")?;
    }

    #[cfg(not(target_os = "windows"))]
    {
        webbrowser::open(url).context("Failed to open URL")?;
    }

    Ok(())
}

fn generate_screenshot_filename() -> String {
    let now = chrono::Local::now();
    format!("screenshot_{}.png", now.format("%Y-%m-%d_%H%M%S"))
}

fn save_image_to_temp(image_data: &arboard::ImageData, filename: &str) -> Result<PathBuf> {
    let temp_dir = std::env::temp_dir();
    let temp_path = temp_dir.join(filename);
    
    let image = image::RgbaImage::from_raw(
        image_data.width as u32,
        image_data.height as u32,
        image_data.bytes.to_vec(),
    )
    .context("Failed to create image from clipboard data")?;
    
    image.save(&temp_path)
        .context("Failed to save image to temp file")?;
    
    Ok(temp_path)
}
