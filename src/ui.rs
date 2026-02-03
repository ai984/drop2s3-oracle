use anyhow::{Context, Result};
use eframe::egui;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::runtime::Handle;

use crate::embedded_icons::IconType;
use crate::history::History;
use crate::tray::{MenuAction, TrayManager};
use crate::upload::{S3Client, UploadManager, UploadProgress};

pub struct UiManager;

impl UiManager {
    pub fn run() -> Result<()> {
        let rt = tokio::runtime::Runtime::new().context("Failed to create tokio runtime")?;
        let handle = rt.handle().clone();
        
        let (upload_manager, progress_rx, cancel_token) = initialize_upload_manager(&rt)?;
        let upload_manager = Arc::new(upload_manager);

        let tray_manager = TrayManager::new()
            .context("Failed to create system tray")?;

        let history_path = crate::utils::get_exe_dir().join("history.json");
        let history = History::new(&history_path)
            .context("Failed to load history")?;

        let options = eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_inner_size([300.0, 400.0])
                .with_min_inner_size([280.0, 350.0])
                .with_position(egui::pos2(9999.0, 9999.0))
                .with_always_on_top()
                .with_resizable(true)
                .with_decorations(true)
                .with_close_button(true)
                .with_minimize_button(true)
                .with_maximize_button(false),
            ..Default::default()
        };

        let _rt_guard = rt;

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
                    history: Arc::new(history),
                    copy_feedback: None,
                    is_uploading: false,
                    rt_handle: handle,
                    should_exit: false,
                })
            }),
        )
        .map_err(|e| anyhow::anyhow!("eframe error: {}", e))?;

        Ok(())
    }
}

fn initialize_upload_manager(rt: &tokio::runtime::Runtime) -> Result<(
    UploadManager,
    tokio::sync::mpsc::UnboundedReceiver<UploadProgress>,
    tokio_util::sync::CancellationToken,
)> {
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

    Ok((upload_manager, progress_rx, cancel_token))
}

struct DropZoneApp {
    tray_manager: TrayManager,
    upload_manager: Arc<UploadManager>,
    progress_rx: tokio::sync::mpsc::UnboundedReceiver<UploadProgress>,
    cancel_token: tokio_util::sync::CancellationToken,
    current_upload: Option<UploadProgress>,
    history: Arc<History>,
    copy_feedback: Option<(String, Instant)>,
    is_uploading: bool,
    rt_handle: Handle,
    should_exit: bool,
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
                    self.should_exit = true;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
                MenuAction::ShowWindow => {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                    ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                }
                MenuAction::ShowSettings => {
                    tracing::info!("Settings not implemented yet");
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
                        if let Err(e) = self.tray_manager.set_icon(IconType::Uploading) {
                            tracing::error!("Failed to set uploading icon: {}", e);
                        }
                    }
                    self.current_upload = Some(progress);
                }
                UploadStatus::Completed | UploadStatus::Failed(_) | UploadStatus::Cancelled => {
                    if self.is_uploading {
                        self.is_uploading = false;
                        if let Err(e) = self.tray_manager.set_icon(IconType::Normal) {
                            tracing::error!("Failed to restore icon: {}", e);
                        }
                    }
                    self.current_upload = None;
                }
                _ => {
                    self.current_upload = Some(progress);
                }
            }
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            let is_hovering = ctx.input(|i| !i.raw.hovered_files.is_empty());
            
            if is_hovering {
                ui.visuals_mut().widgets.noninteractive.bg_fill =
                    egui::Color32::from_rgb(100, 180, 100);
            }

            ui.vertical_centered(|ui| {
                ui.add_space(30.0);
                ui.heading("☁️");
                ui.add_space(10.0);
                if is_hovering {
                    ui.label("Upusc tutaj");
                } else {
                    ui.label("Przeciagnij plik");
                }
            });

            ui.add_space(20.0);
            ui.separator();

            if let Some(progress) = &self.current_upload {
                use crate::upload::UploadStatus;
                
                ui.add_space(10.0);
                ui.label(&progress.filename);
                
                let fraction = if progress.total_bytes > 0 {
                    progress.bytes_uploaded as f32 / progress.total_bytes as f32
                } else {
                    0.0
                };
                
                ui.add(egui::ProgressBar::new(fraction).show_percentage());
                
                let status_text = match &progress.status {
                    UploadStatus::Queued => "W kolejce...",
                    UploadStatus::Uploading => "Przesylanie...",
                    UploadStatus::Completed => "Ukonczone",
                    UploadStatus::Failed(err) => err,
                    UploadStatus::Cancelled => "Anulowano",
                };
                ui.small(status_text);
                
                if matches!(progress.status, UploadStatus::Queued | UploadStatus::Uploading)
                    && ui.small_button("Anuluj").clicked()
                {
                    self.cancel_token.cancel();
                }
                
                ui.add_space(10.0);
                ui.separator();
            }

            ui.add_space(10.0);
            ui.label("Historia:");
            ui.add_space(5.0);
            
            let entries = self.history.get_all();
            
            if entries.is_empty() {
                ui.small("Brak plikow");
            } else {
                egui::ScrollArea::vertical()
                    .max_height(150.0)
                    .show(ui, |ui| {
                        for entry in entries.iter().take(10) {
                            ui.horizontal(|ui| {
                                let display_name = if entry.filename.len() > 20 {
                                    format!("{}...", &entry.filename[..17])
                                } else {
                                    entry.filename.clone()
                                };
                                
                                let response = ui.small(&display_name);
                                if response.double_clicked() {
                                    if let Err(e) = open_url_in_browser(&entry.url) {
                                        tracing::error!("Failed to open URL: {}", e);
                                    }
                                }
                                response.on_hover_text(&entry.url);
                                
                                if ui.small_button("📋").on_hover_text("Kopiuj link").clicked() {
                                    if let Ok(mut clipboard) = arboard::Clipboard::new() {
                                        if clipboard.set_text(entry.url.clone()).is_ok() {
                                            self.copy_feedback = Some((entry.filename.clone(), Instant::now()));
                                        }
                                    }
                                }
                            });
                        }
                    });
            }

            if let Some((filename, instant)) = &self.copy_feedback {
                if instant.elapsed() < Duration::from_secs(2) {
                    ui.add_space(5.0);
                    ui.colored_label(egui::Color32::GREEN, format!("Skopiowano: {}", filename));
                } else {
                    self.copy_feedback = None;
                }
            }
        });

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
            let history = self.history.clone();
            self.rt_handle.spawn(async move {
                match manager.upload_files(dropped_files).await {
                    Ok(urls) => {
                        tracing::info!("Upload completed: {} files", urls.len());
                        for url in &urls {
                            tracing::info!("  - {}", url);
                            if let Some(filename) = url.split('/').next_back() {
                                history.add(filename, url);
                            }
                        }
                        if let Some(first_url) = urls.first() {
                            if let Ok(mut clipboard) = arboard::Clipboard::new() {
                                let _ = clipboard.set_text(first_url.clone());
                            }
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
            if let Ok(mut clipboard) = arboard::Clipboard::new() {
                if let Ok(image_data) = clipboard.get_image() {
                    let filename = generate_screenshot_filename();
                    if let Ok(temp_path) = save_image_to_temp(&image_data, &filename) {
                        let manager = self.upload_manager.clone();
                        let history = self.history.clone();
                        self.rt_handle.spawn(async move {
                            match manager.upload_files(vec![temp_path.clone()]).await {
                                Ok(urls) => {
                                    if let Some(url) = urls.first() {
                                        tracing::info!("Screenshot uploaded: {}", url);
                                        history.add(&filename, url);
                                        if let Ok(mut clipboard) = arboard::Clipboard::new() {
                                            let _ = clipboard.set_text(url.clone());
                                        }
                                    }
                                    let _ = std::fs::remove_file(&temp_path);
                                }
                                Err(e) => {
                                    tracing::error!("Screenshot upload failed: {}", e);
                                    let _ = std::fs::remove_file(&temp_path);
                                }
                            }
                        });
                    }
                }
            }
        }

        if ctx.input(|i| i.viewport().close_requested()) && !self.should_exit {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
        }

        ctx.request_repaint();
    }
}

fn open_url_in_browser(url: &str) -> Result<()> {
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("cmd")
            .args(["/C", "start", "", url])
            .spawn()
            .context("Failed to open URL")?;
    }

    #[cfg(not(target_os = "windows"))]
    {
        std::process::Command::new("xdg-open")
            .arg(url)
            .spawn()
            .context("Failed to open URL")?;
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
