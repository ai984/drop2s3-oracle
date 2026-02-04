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
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .context("Failed to create tokio runtime")?;
        let handle = rt.handle().clone();
        
        let (upload_manager, progress_rx, cancel_token) = initialize_upload_manager(&rt)?;
        let upload_manager = Arc::new(upload_manager);

        let tray_manager = TrayManager::new()
            .context("Failed to create system tray")?;

        let history_path = crate::utils::get_exe_dir().join("history.json");
        let history = History::new(&history_path)
            .context("Failed to load history")?;

        let window_size = [320.0_f32, 290.0_f32];
        let position = get_bottom_right_position(window_size[0], window_size[1]);
        
        let mut viewport = egui::ViewportBuilder::default()
            .with_inner_size(window_size)
            .with_min_inner_size([280.0, 200.0])
            .with_always_on_top()
            .with_resizable(true)
            .with_decorations(true)
            .with_close_button(true)
            .with_minimize_button(true)
            .with_maximize_button(false)
            .with_transparent(false);
        
        if let Some(pos) = position {
            viewport = viewport.with_position(pos);
        }
        
        let options = eframe::NativeOptions {
            viewport,
            ..Default::default()
        };

        let _rt_guard = rt;

        eframe::run_native(
            "Drop2S3",
            options,
            Box::new(move |_cc| {
                Ok(Box::new(DropZoneApp {
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
                    upload_started_at: None,
                    last_error: Arc::new(std::sync::Mutex::new(None)),
                }))
            }),
        )
        .map_err(|e| anyhow::anyhow!("eframe error: {e}"))?;

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
    upload_started_at: Option<Instant>,
    /// Last error message with timestamp for auto-clear
    last_error: Arc<std::sync::Mutex<Option<(String, Instant)>>>,
}

impl eframe::App for DropZoneApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Check tray quit request
        if TrayManager::quit_requested() {
            self.should_exit = true;
            self.cancel_token.cancel();  // Signal uploads to stop
        }

        if self.should_exit {
            // Give uploads time to cancel gracefully
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;  // Exit update loop
        }
        
        if TrayManager::should_show_window() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        }

        if let Some(event) = TrayManager::poll_menu_event() {
            let action = self.tray_manager.handle_menu_event(&event);

            match action {
                MenuAction::Quit => {}
                MenuAction::ShowWindow => {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                    ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
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
                        self.upload_started_at = Some(Instant::now());
                        if let Err(e) = self.tray_manager.set_icon(IconType::Uploading) {
                            tracing::error!("Failed to set uploading icon: {}", e);
                        }
                    }
                    self.current_upload = Some(progress);
                }
                UploadStatus::Completed | UploadStatus::Failed(_) | UploadStatus::Cancelled => {
                    if self.is_uploading {
                        self.is_uploading = false;
                        self.upload_started_at = None;
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
                ui.heading("☁");
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
                    UploadStatus::Queued => "W kolejce...".to_string(),
                    UploadStatus::Uploading => {
                        if let Some(started) = self.upload_started_at {
                            let elapsed = started.elapsed().as_secs_f64();
                            if elapsed > 0.5 && progress.bytes_uploaded > 0 {
                                let speed = progress.bytes_uploaded as f64 / elapsed;
                                format!("{} | {}", format_speed(speed), format_size(progress.bytes_uploaded))
                            } else {
                                "Przesylanie...".to_string()
                            }
                        } else {
                            "Przesylanie...".to_string()
                        }
                    }
                    UploadStatus::Completed => "Ukonczone".to_string(),
                    UploadStatus::Failed(err) => err.clone(),
                    UploadStatus::Cancelled => "Anulowano".to_string(),
                };
                ui.small(&status_text);
                
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
                for (idx, entry) in entries.iter().take(5).enumerate() {
                    let is_fresh = idx == 0 && {
                        let age = chrono::Utc::now().signed_duration_since(entry.timestamp);
                        age.num_seconds() < 30
                    };
                    let mut url_display = format_url_short(&entry.url);
                    
                    ui.horizontal(|ui| {
                        let available = ui.available_width() - 30.0;
                        
                        let text_edit = egui::TextEdit::singleline(&mut url_display)
                            .interactive(false)
                            .font(egui::TextStyle::Small);
                        
                        let response = if is_fresh {
                            ui.visuals_mut().widgets.inactive.bg_fill = egui::Color32::from_rgb(45, 65, 95);
                            ui.visuals_mut().widgets.inactive.fg_stroke.color = egui::Color32::from_rgb(150, 200, 255);
                            ui.add_sized([available, 20.0], text_edit)
                        } else {
                            ui.add_sized([available, 18.0], text_edit)
                        };
                        
                        if response.clicked() {
                            if let Ok(mut clipboard) = arboard::Clipboard::new() {
                                if clipboard.set_text(entry.url.clone()).is_ok() {
                                    self.copy_feedback = Some((entry.filename.clone(), Instant::now()));
                                }
                            }
                        }
                        if response.double_clicked() {
                            if let Err(e) = open_url_in_browser(&entry.url) {
                                tracing::error!("Failed to open URL: {}", e);
                            }
                        }
                        
                        if ui.small_button("📋").on_hover_text("Kopiuj").clicked() {
                            if let Ok(mut clipboard) = arboard::Clipboard::new() {
                                if clipboard.set_text(entry.url.clone()).is_ok() {
                                    self.copy_feedback = Some((entry.filename.clone(), Instant::now()));
                                }
                            }
                        }
                    });
                }
            }

            if let Some((filename, instant)) = &self.copy_feedback {
                if instant.elapsed() < Duration::from_secs(2) {
                    ui.add_space(5.0);
                    ui.colored_label(egui::Color32::GREEN, format!("Skopiowano: {filename}"));
                } else {
                    self.copy_feedback = None;
                }
            }

            if let Ok(mut err) = self.last_error.lock() {
                if let Some((msg, timestamp)) = err.as_ref() {
                    if timestamp.elapsed() < Duration::from_secs(10) {
                        ui.add_space(5.0);
                        ui.colored_label(egui::Color32::from_rgb(255, 100, 100), msg);
                    } else {
                        *err = None;
                    }
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
            let error_state = self.last_error.clone();
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
                        if let Ok(mut err) = error_state.lock() {
                            *err = Some((format!("Upload failed: {e}"), Instant::now()));
                        }
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
                        let error_state = self.last_error.clone();
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
                                     let _ = tokio::fs::remove_file(&temp_path).await;
                                 }
                                 Err(e) => {
                                     tracing::error!("Screenshot upload failed: {}", e);
                                     let _ = tokio::fs::remove_file(&temp_path).await;
                                     if let Ok(mut err) = error_state.lock() {
                                         *err = Some((format!("Screenshot failed: {e}"), Instant::now()));
                                     }
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

        let has_error = self.last_error.lock().map(|e| e.is_some()).unwrap_or(false);
        if self.is_uploading || self.copy_feedback.is_some() || has_error {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
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

#[cfg(target_os = "windows")]
fn get_bottom_right_position(window_width: f32, window_height: f32) -> Option<egui::Pos2> {
    use windows::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CXSCREEN, SM_CYSCREEN};
    let screen_width = unsafe { GetSystemMetrics(SM_CXSCREEN) };
    let screen_height = unsafe { GetSystemMetrics(SM_CYSCREEN) };
    tracing::info!(
        "Screen size: {}x{}, window: {}x{}",
        screen_width, screen_height, window_width, window_height
    );
    if screen_width > 0 && screen_height > 0 {
        let margin = 20.0;
        let taskbar_height = 60.0;
        let x = screen_width as f32 - window_width - margin;
        let y = screen_height as f32 - window_height - margin - taskbar_height;
        tracing::info!("Calculated position: ({}, {})", x, y);
        Some(egui::pos2(x, y))
    } else {
        None
    }
}

#[cfg(not(target_os = "windows"))]
fn get_bottom_right_position(_window_width: f32, _window_height: f32) -> Option<egui::Pos2> {
    None
}

fn format_speed(bytes_per_sec: f64) -> String {
    if bytes_per_sec >= 1_000_000.0 {
        format!("{:.1} MB/s", bytes_per_sec / 1_000_000.0)
    } else if bytes_per_sec >= 1_000.0 {
        format!("{:.0} KB/s", bytes_per_sec / 1_000.0)
    } else {
        format!("{bytes_per_sec:.0} B/s")
    }
}

fn format_size(bytes: u64) -> String {
    if bytes >= 1_000_000_000 {
        format!("{:.1} GB", bytes as f64 / 1_000_000_000.0)
    } else if bytes >= 1_000_000 {
        format!("{:.1} MB", bytes as f64 / 1_000_000.0)
    } else if bytes >= 1_000 {
        format!("{:.0} KB", bytes as f64 / 1_000.0)
    } else {
        format!("{bytes} B")
    }
}

fn format_url_short(url: &str) -> String {
    let without_protocol = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .unwrap_or(url);
    
    let parts: Vec<&str> = without_protocol.split('/').collect();
    if parts.len() >= 2 {
        let domain = parts.first().unwrap_or(&"");
        let domain_short: String = domain.chars().take(20).collect();
        let filename = parts.last().unwrap_or(&"");
        return format!("{domain_short}.../{filename}");
    }
    
    without_protocol.to_string()
}
