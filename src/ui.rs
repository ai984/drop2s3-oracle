use anyhow::{Context, Result};
use eframe::egui;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::embedded_icons::IconType;
use crate::tray::TrayManager;
use crate::upload::UploadProgress;
use crate::AppState;

const WINDOW_SIZE: [f32; 2] = [320.0, 290.0];

#[derive(Clone, PartialEq)]
pub enum UpdateState {
    Checking,
    #[allow(dead_code)]
    Available(String),
    Downloading,
    ReadyToInstall,
    None,
}

pub fn show_window(app_state: Arc<AppState>) -> Result<()> {
    tracing::info!("show_window called");

    let position = get_validated_position(&app_state);

    let viewport = egui::ViewportBuilder::default()
        .with_inner_size(WINDOW_SIZE)
        .with_min_inner_size([280.0, 200.0])
        .with_always_on_top()
        .with_resizable(true)
        .with_decorations(true)
        .with_close_button(true)
        .with_minimize_button(true)
        .with_maximize_button(false)
        .with_position(position);

    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    eframe::run_native(
        "Drop2S3",
        options,
        Box::new(move |_cc| Ok(Box::new(DropZoneApp::new(app_state)))),
    )
    .map_err(|e| anyhow::anyhow!("eframe error: {e}"))?;

    Ok(())
}

fn get_validated_position(app_state: &AppState) -> egui::Pos2 {
    let (saved_x, saved_y) = {
        let config = app_state.config.lock().unwrap();
        (config.app.window_x, config.app.window_y)
    };

    let (screen_w, screen_h) = get_screen_size();
    
    if let (Some(x), Some(y)) = (saved_x, saved_y) {
        if is_position_visible(x, y, screen_w, screen_h) {
            tracing::info!("Using saved position: ({}, {})", x, y);
            return egui::pos2(x, y);
        }
        tracing::warn!("Saved position ({}, {}) is off-screen, using default", x, y);
    }

    get_bottom_right_position(screen_w, screen_h)
}

fn is_position_visible(x: f32, y: f32, screen_w: f32, screen_h: f32) -> bool {
    let min_visible = 100.0;
    x >= -WINDOW_SIZE[0] + min_visible 
        && x <= screen_w - min_visible
        && y >= 0.0
        && y <= screen_h - min_visible
}

fn get_bottom_right_position(screen_w: f32, screen_h: f32) -> egui::Pos2 {
    let margin = 20.0;
    let taskbar_height = 50.0;
    let x = screen_w - WINDOW_SIZE[0] - margin;
    let y = screen_h - WINDOW_SIZE[1] - margin - taskbar_height;
    tracing::info!("Using bottom-right position: ({}, {})", x, y);
    egui::pos2(x.max(0.0), y.max(0.0))
}

#[cfg(target_os = "windows")]
fn get_screen_size() -> (f32, f32) {
    use windows::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CXSCREEN, SM_CYSCREEN};
    let w = unsafe { GetSystemMetrics(SM_CXSCREEN) } as f32;
    let h = unsafe { GetSystemMetrics(SM_CYSCREEN) } as f32;
    (w.max(800.0), h.max(600.0))
}

#[cfg(not(target_os = "windows"))]
fn get_screen_size() -> (f32, f32) {
    (1920.0, 1080.0)
}

struct DropZoneApp {
    app_state: Arc<AppState>,
    current_upload: Option<UploadProgress>,
    copy_feedback: Option<(String, Instant)>,
    is_uploading: bool,
    should_exit: bool,
    upload_started_at: Option<Instant>,
    last_error: Arc<std::sync::Mutex<Option<(String, Instant)>>>,
    upload_queue: HashMap<String, UploadProgress>,
    total_files_count: usize,
    completed_files_count: usize,
    last_window_pos: Option<egui::Pos2>,
}

impl DropZoneApp {
    fn new(app_state: Arc<AppState>) -> Self {
        Self {
            app_state,
            current_upload: None,
            copy_feedback: None,
            is_uploading: false,
            should_exit: false,
            upload_started_at: None,
            last_error: Arc::new(std::sync::Mutex::new(None)),
            upload_queue: HashMap::new(),
            total_files_count: 0,
            completed_files_count: 0,
            last_window_pos: None,
        }
    }
}

impl eframe::App for DropZoneApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if TrayManager::quit_requested() {
            tracing::info!("Quit requested via tray");
            self.should_exit = true;
            self.app_state.upload_manager.cancel();
        }

        while let Some(event) = TrayManager::poll_menu_event() {
            if let Ok(tray) = self.app_state.tray_manager.lock() {
                use crate::tray::MenuAction;
                match tray.handle_menu_event(&event) {
                    MenuAction::Quit => {
                        tracing::info!("Quit from menu event");
                        self.should_exit = true;
                        self.app_state.upload_manager.cancel();
                    }
                    _ => {}
                }
            }
        }

        if self.should_exit {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }

        self.process_upload_events();

        egui::CentralPanel::default().show(ctx, |ui| {
            self.render_update_status(ui);
            self.render_drop_zone(ctx, ui);
            self.render_upload_progress(ui);
            self.render_history(ui);
            self.render_feedback(ui);
            self.render_version(ui);
        });

        self.handle_dropped_files(ctx);
        self.handle_clipboard_paste(ctx);
        self.handle_close_request(ctx);
        self.schedule_repaint(ctx);
    }
}

impl DropZoneApp {
    fn process_upload_events(&mut self) {
        let Ok(mut rx) = self.app_state.progress_rx.lock() else {
            return;
        };

        while let Ok(progress) = rx.try_recv() {
            use crate::upload::UploadStatus;

            match &progress.status {
                UploadStatus::Queued => {
                    self.upload_queue
                        .insert(progress.file_id.clone(), progress.clone());
                    self.total_files_count = self.upload_queue.len();

                    if !self.is_uploading {
                        self.is_uploading = true;
                        self.upload_started_at = Some(Instant::now());
                        if let Ok(mut tray) = self.app_state.tray_manager.lock() {
                            if let Err(e) = tray.set_icon(IconType::Uploading) {
                                tracing::error!("Failed to set uploading icon: {}", e);
                            }
                        }
                    }
                }
                UploadStatus::Uploading => {
                    self.upload_queue
                        .insert(progress.file_id.clone(), progress.clone());
                    self.current_upload = Some(progress);
                }
                UploadStatus::Completed | UploadStatus::Failed(_) | UploadStatus::Cancelled => {
                    self.upload_queue.remove(&progress.file_id);
                    self.completed_files_count += 1;

                    if self.upload_queue.is_empty() {
                        self.is_uploading = false;
                        self.upload_started_at = None;
                        self.current_upload = None;
                        self.total_files_count = 0;
                        self.completed_files_count = 0;
                        self.app_state.upload_manager.reset_cancel();
                        if let Ok(mut tray) = self.app_state.tray_manager.lock() {
                            if let Err(e) = tray.set_icon(IconType::Normal) {
                                tracing::error!("Failed to restore icon: {}", e);
                            }
                        }
                    }
                }
            }
        }
    }

    fn render_update_status(&self, ui: &mut egui::Ui) {
        if let Ok(state) = self.app_state.update_state.lock() {
            match &*state {
                UpdateState::Downloading => {
                    ui.colored_label(egui::Color32::YELLOW, "Pobieranie aktualizacji...");
                    ui.separator();
                }
                UpdateState::ReadyToInstall => {
                    ui.colored_label(
                        egui::Color32::from_rgb(100, 200, 100),
                        "Aktualizacja pobrana - uruchom ponownie aplikacje",
                    );
                    ui.separator();
                }
                _ => {}
            }
        }
    }

    fn render_drop_zone(&self, ctx: &egui::Context, ui: &mut egui::Ui) {
        let is_hovering = ctx.input(|i| !i.raw.hovered_files.is_empty());

        if is_hovering {
            ui.visuals_mut().widgets.noninteractive.bg_fill = egui::Color32::from_rgb(100, 180, 100);
        }

        ui.vertical_centered(|ui| {
            ui.add_space(20.0);
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
    }

    fn render_upload_progress(&mut self, ui: &mut egui::Ui) {
        if !self.is_uploading || self.total_files_count == 0 {
            return;
        }

        ui.add_space(10.0);

        let total_bytes: u64 = self.upload_queue.values().map(|p| p.total_bytes).sum();
        let uploaded_bytes: u64 = self.upload_queue.values().map(|p| p.bytes_uploaded).sum();
        let fraction = if total_bytes > 0 {
            uploaded_bytes as f32 / total_bytes as f32
        } else {
            0.0
        };

        if self.total_files_count > 1 {
            ui.label(format!(
                "Przesylanie {}/{} plikow...",
                self.completed_files_count + 1,
                self.total_files_count
            ));
        }

        ui.add(egui::ProgressBar::new(fraction).show_percentage());

        if let Some(progress) = &self.current_upload {
            let status_text = if let Some(started) = self.upload_started_at {
                let elapsed = started.elapsed().as_secs_f64();
                if elapsed > 0.5 && uploaded_bytes > 0 {
                    let speed = uploaded_bytes as f64 / elapsed;
                    format!("{} - {}", progress.filename, format_speed(speed))
                } else {
                    progress.filename.clone()
                }
            } else {
                progress.filename.clone()
            };
            ui.small(&status_text);
        }

        if ui.small_button("Anuluj").clicked() {
            self.app_state.upload_manager.cancel();
        }

        ui.add_space(10.0);
        ui.separator();
    }

    fn render_history(&mut self, ui: &mut egui::Ui) {
        ui.add_space(10.0);

        let entries = self.app_state.history.get_all();
        let fresh_entries: Vec<_> = entries
            .iter()
            .filter(|e| {
                let age = chrono::Utc::now().signed_duration_since(e.timestamp);
                age.num_seconds() < 30
            })
            .collect();

        ui.horizontal(|ui| {
            ui.label("Historia:");
            if fresh_entries.len() > 1 {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button("Kopiuj wszystkie").clicked() {
                        let all_urls: String = fresh_entries
                            .iter()
                            .map(|e| e.url.as_str())
                            .collect::<Vec<_>>()
                            .join("\n");
                        if let Ok(mut clipboard) = arboard::Clipboard::new() {
                            if clipboard.set_text(all_urls).is_ok() {
                                self.copy_feedback = Some((
                                    format!("{} linkow", fresh_entries.len()),
                                    Instant::now(),
                                ));
                            }
                        }
                    }
                });
            }
        });
        ui.add_space(5.0);

        if entries.is_empty() {
            ui.small("Brak plikow");
        } else {
            for entry in entries.iter().take(5) {
                let age = chrono::Utc::now().signed_duration_since(entry.timestamp);
                let is_fresh = age.num_seconds() < 30;
                let mut url_display = format_url_short(&entry.url, &entry.filename);

                ui.horizontal(|ui| {
                    let available = ui.available_width() - 30.0;

                    let text_edit = egui::TextEdit::singleline(&mut url_display)
                        .interactive(false)
                        .font(egui::TextStyle::Small);

                    let response = if is_fresh {
                        ui.visuals_mut().widgets.inactive.bg_fill =
                            egui::Color32::from_rgb(45, 65, 95);
                        ui.visuals_mut().widgets.inactive.fg_stroke.color =
                            egui::Color32::from_rgb(150, 200, 255);
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
    }

    fn render_feedback(&mut self, ui: &mut egui::Ui) {
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
    }

    fn render_version(&self, ui: &mut egui::Ui) {
        ui.with_layout(egui::Layout::bottom_up(egui::Align::RIGHT), |ui| {
            ui.label(
                egui::RichText::new(concat!("v", env!("CARGO_PKG_VERSION")))
                    .small()
                    .color(egui::Color32::from_gray(120)),
            );
        });
    }

    fn handle_dropped_files(&self, ctx: &egui::Context) {
        let dropped_files: Vec<PathBuf> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .filter_map(|f| f.path.as_ref())
                .filter(|p| p.is_file())
                .cloned()
                .collect()
        });

        if dropped_files.is_empty() {
            return;
        }

        tracing::info!("Files dropped: {} files", dropped_files.len());
        let manager = self.app_state.upload_manager.clone();
        let history = self.app_state.history.clone();
        let error_state = self.last_error.clone();

        self.app_state.rt_handle.spawn(async move {
            match manager.upload_files(dropped_files).await {
                Ok(results) => {
                    tracing::info!("Upload completed: {} files", results.len());
                    for (filename, url) in &results {
                        tracing::info!("  - {}", url);
                        history.add(filename, url);
                    }
                    if let Some((_, first_url)) = results.first() {
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

    fn handle_clipboard_paste(&self, ctx: &egui::Context) {
        let ctrl_v_pressed = ctx.input(|i| i.key_pressed(egui::Key::V) && i.modifiers.ctrl);
        if !ctrl_v_pressed {
            return;
        }

        if let Ok(mut clipboard) = arboard::Clipboard::new() {
            if let Ok(image_data) = clipboard.get_image() {
                let filename = generate_screenshot_filename();
                if let Ok(temp_path) = save_image_to_temp(&image_data, &filename) {
                    let manager = self.app_state.upload_manager.clone();
                    let history = self.app_state.history.clone();
                    let error_state = self.last_error.clone();

                    self.app_state.rt_handle.spawn(async move {
                        match manager.upload_files(vec![temp_path.clone()]).await {
                            Ok(results) => {
                                if let Some((_, url)) = results.first() {
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

    fn handle_close_request(&mut self, ctx: &egui::Context) {
        self.last_window_pos = ctx.input(|i| i.viewport().outer_rect).map(|r| r.min);

        if ctx.input(|i| i.viewport().close_requested()) {
            if let Some(pos) = self.last_window_pos {
                tracing::info!("Saving window position: ({}, {})", pos.x, pos.y);
                if let Ok(mut config) = self.app_state.config.lock() {
                    config.app.window_x = Some(pos.x);
                    config.app.window_y = Some(pos.y);
                    let _ = config.save(&self.app_state.config_path);
                }
            }
        }
    }

    fn schedule_repaint(&self, ctx: &egui::Context) {
        let has_error = self
            .last_error
            .lock()
            .map(|e| e.is_some())
            .unwrap_or(false);
        let is_updating = self
            .app_state
            .update_state
            .lock()
            .map(|s| matches!(*s, UpdateState::Downloading))
            .unwrap_or(false);

        if self.is_uploading || is_updating {
            ctx.request_repaint_after(Duration::from_millis(100));
        } else if self.copy_feedback.is_some() || has_error {
            ctx.request_repaint_after(Duration::from_millis(500));
        } else {
            ctx.request_repaint_after(Duration::from_secs(1));
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

    image
        .save(&temp_path)
        .context("Failed to save image to temp file")?;

    Ok(temp_path)
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

#[allow(dead_code)]
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

fn format_url_short(url: &str, original_filename: &str) -> String {
    let without_protocol = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .unwrap_or(url);

    let parts: Vec<&str> = without_protocol.split('/').collect();
    if parts.len() >= 2 {
        let domain = parts.first().unwrap_or(&"");
        let domain_short: String = domain.chars().take(20).collect();
        return format!("{domain_short}.../{original_filename}");
    }

    without_protocol.to_string()
}
