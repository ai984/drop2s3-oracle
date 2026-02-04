use anyhow::{Context, Result};
use eframe::egui;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::runtime::Handle;

#[cfg(target_os = "windows")]
use std::sync::OnceLock;

use crate::embedded_icons::IconType;
use crate::history::History;
use crate::tray::{MenuAction, TrayManager};
use crate::update::UpdateManager;
use crate::upload::{S3Client, UploadManager, UploadProgress};

#[cfg(target_os = "windows")]
static WINDOW_HWND: OnceLock<isize> = OnceLock::new();

#[cfg(target_os = "windows")]
pub fn hide_window() {
    use windows::Win32::UI::WindowsAndMessaging::{ShowWindow, SW_HIDE};
    use windows::Win32::Foundation::HWND;
    
    if let Some(&hwnd) = WINDOW_HWND.get() {
        unsafe {
            let _ = ShowWindow(HWND(hwnd as *mut _), SW_HIDE);
        }
    }
}

#[cfg(target_os = "windows")]
pub fn show_window() {
    use windows::Win32::UI::WindowsAndMessaging::{ShowWindow, SetForegroundWindow, SW_SHOW};
    use windows::Win32::Foundation::HWND;
    
    if let Some(&hwnd) = WINDOW_HWND.get() {
        unsafe {
            let handle = HWND(hwnd as *mut _);
            let _ = ShowWindow(handle, SW_SHOW);
            let _ = SetForegroundWindow(handle);
        }
    }
}

#[cfg(not(target_os = "windows"))]
pub fn hide_window() {}

#[cfg(not(target_os = "windows"))]
pub fn show_window() {}

pub struct UiManager;

impl UiManager {
    pub fn run() -> Result<()> {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .context("Failed to create tokio runtime")?;
        let handle = rt.handle().clone();
        
        let (upload_manager, progress_rx) = initialize_upload_manager(&rt)?;
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

        eframe::run_native(
            "Drop2S3",
            options,
            Box::new(move |cc| {
                #[cfg(target_os = "windows")]
                {
                    use raw_window_handle::HasWindowHandle;
                    if let Ok(handle) = cc.window_handle() {
                        if let raw_window_handle::RawWindowHandle::Win32(win32) = handle.as_raw() {
                            let hwnd = win32.hwnd.get() as isize;
                            let _ = WINDOW_HWND.set(hwnd);
                            tracing::info!("Captured window HWND: {}", hwnd);
                        }
                    }
                }
                let _ = cc;
                
                let update_state = Arc::new(std::sync::Mutex::new(UpdateState::Checking));
                
                let update_state_clone = update_state.clone();
                handle.spawn(async move {
                    let manager = UpdateManager::new();
                    match manager.check_for_updates().await {
                        Ok(Some(version)) => {
                            if let Ok(mut state) = update_state_clone.lock() {
                                *state = UpdateState::Downloading;
                            }
                            match manager.download_update(&version).await {
                                Ok(()) => {
                                    if let Ok(mut state) = update_state_clone.lock() {
                                        *state = UpdateState::ReadyToInstall;
                                    }
                                }
                                Err(_) => {
                                    if let Ok(mut state) = update_state_clone.lock() {
                                        *state = UpdateState::None;
                                    }
                                }
                            }
                        }
                        Ok(None) | Err(_) => {
                            if let Ok(mut state) = update_state_clone.lock() {
                                *state = UpdateState::None;
                            }
                        }
                    }
                });

                Ok(Box::new(DropZoneApp {
                    tray_manager,
                    upload_manager,
                    progress_rx,
                    current_upload: None,
                    history: Arc::new(history),
                    copy_feedback: None,
                    is_uploading: false,
                    rt_handle: handle,
                    should_exit: false,
                    upload_started_at: None,
                    last_error: Arc::new(std::sync::Mutex::new(None)),
                    update_state,
                    upload_queue: HashMap::new(),
                    total_files_count: 0,
                    completed_files_count: 0,
                    window_visible: true,
                }))
            }),
        )
        .map_err(|e| anyhow::anyhow!("eframe error: {e}"))?;

        rt.shutdown_timeout(std::time::Duration::from_millis(500));

        Ok(())
    }
}

fn initialize_upload_manager(rt: &tokio::runtime::Runtime) -> Result<(
    UploadManager,
    tokio::sync::mpsc::UnboundedReceiver<UploadProgress>,
)> {
    let config_path = crate::utils::get_exe_dir().join("config.toml");
    let config = crate::config::Config::load(&config_path)
        .context("Failed to load config")?;

    let s3_client = rt
        .block_on(S3Client::new(&config))
        .context("Failed to create S3 client")?;

    let (upload_manager, progress_rx) = UploadManager::new(
        s3_client,
        config.advanced.parallel_uploads as usize,
        3,
    );

    Ok((upload_manager, progress_rx))
}

#[derive(Clone, PartialEq)]
enum UpdateState {
    Checking,
    Available(String),
    Downloading,
    ReadyToInstall,
    None,
}

struct DropZoneApp {
    tray_manager: TrayManager,
    upload_manager: Arc<UploadManager>,
    progress_rx: tokio::sync::mpsc::UnboundedReceiver<UploadProgress>,
    current_upload: Option<UploadProgress>,
    history: Arc<History>,
    copy_feedback: Option<(String, Instant)>,
    is_uploading: bool,
    rt_handle: Handle,
    should_exit: bool,
    upload_started_at: Option<Instant>,
    last_error: Arc<std::sync::Mutex<Option<(String, Instant)>>>,
    update_state: Arc<std::sync::Mutex<UpdateState>>,
    upload_queue: HashMap<String, UploadProgress>,
    total_files_count: usize,
    completed_files_count: usize,
    window_visible: bool,
}

impl eframe::App for DropZoneApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Check tray quit request
        if TrayManager::quit_requested() {
            self.should_exit = true;
            self.upload_manager.cancel();
        }

        if self.should_exit {
            // Give uploads time to cancel gracefully
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;  // Exit update loop
        }
        
        if TrayManager::should_show_window() {
            self.window_visible = true;
            show_window();
        }

        if let Some(event) = TrayManager::poll_menu_event() {
            let action = self.tray_manager.handle_menu_event(&event);

            if let MenuAction::ShowWindow = action {
                self.window_visible = true;
                show_window();
            }
        }

        let is_minimized = ctx.input(|i| i.viewport().minimized).unwrap_or(false);
        let is_visible = self.window_visible && !is_minimized;
        
        while let Ok(progress) = self.progress_rx.try_recv() {
            use crate::upload::UploadStatus;
            
            match &progress.status {
                UploadStatus::Queued => {
                    self.upload_queue.insert(progress.file_id.clone(), progress.clone());
                    self.total_files_count = self.upload_queue.len();
                    
                    if !self.is_uploading {
                        self.is_uploading = true;
                        self.upload_started_at = Some(Instant::now());
                        if let Err(e) = self.tray_manager.set_icon(IconType::Uploading) {
                            tracing::error!("Failed to set uploading icon: {}", e);
                        }
                    }
                }
                UploadStatus::Uploading => {
                    self.upload_queue.insert(progress.file_id.clone(), progress.clone());
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
                        self.upload_manager.reset_cancel();
                        if let Err(e) = self.tray_manager.set_icon(IconType::Normal) {
                            tracing::error!("Failed to restore icon: {}", e);
                        }
                    }
                }
            }
        }

        if !is_visible {
            return self.handle_background_events(ctx);
        }
        
        egui::CentralPanel::default().show(ctx, |ui| {
            if let Ok(state) = self.update_state.lock() {
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

            let is_hovering = ctx.input(|i| !i.raw.hovered_files.is_empty());
            
            if is_hovering {
                ui.visuals_mut().widgets.noninteractive.bg_fill =
                    egui::Color32::from_rgb(100, 180, 100);
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

            if self.is_uploading && self.total_files_count > 0 {
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
                    self.upload_manager.cancel();
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

            ui.with_layout(egui::Layout::bottom_up(egui::Align::RIGHT), |ui| {
                ui.label(
                    egui::RichText::new(concat!("v", env!("CARGO_PKG_VERSION")))
                        .small()
                        .color(egui::Color32::from_gray(120)),
                );
            });
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

        if ctx.input(|i| i.viewport().close_requested()) && !self.should_exit {
            self.window_visible = false;
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            hide_window();
        }

        self.schedule_repaint(ctx);
    }
}

impl DropZoneApp {
    fn handle_background_events(&mut self, ctx: &egui::Context) {
        if ctx.input(|i| i.viewport().close_requested()) && !self.should_exit {
            self.window_visible = false;
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            hide_window();
        }
        self.schedule_repaint(ctx);
    }
    
    fn schedule_repaint(&self, ctx: &egui::Context) {
        let is_minimized = ctx.input(|i| i.viewport().minimized).unwrap_or(false);
        let is_hidden = !self.window_visible || is_minimized;
        
        if is_hidden {
            return;
        }
        
        let has_error = self.last_error.lock().map(|e| e.is_some()).unwrap_or(false);
        let is_updating = self
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
