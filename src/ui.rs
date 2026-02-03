use anyhow::{Context, Result};
use eframe::egui;

use crate::tray::{MenuAction, TrayManager};

pub struct UiManager;

impl UiManager {
    pub fn run() -> Result<()> {
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
                })
            }),
        )
        .map_err(|e| anyhow::anyhow!("eframe error: {}", e))?;

        Ok(())
    }
}

struct DropZoneApp {
    tray_manager: TrayManager,
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

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(80.0);
                ui.heading("☁️");
                ui.add_space(20.0);
                ui.label("Upuść plik");
            });
        });

        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            tracing::debug!("ESC pressed, ignoring");
        }

        ctx.request_repaint();
    }
}
