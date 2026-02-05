use anyhow::{Context, Result};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;
use tray_icon::{
    menu::{Menu, MenuEvent, MenuId, MenuItem},
    TrayIcon, TrayIconBuilder, TrayIconEvent,
};

use crate::embedded_icons::{self, IconType};

static QUIT_ITEM_ID: OnceLock<MenuId> = OnceLock::new();
static SHOW_ITEM_ID: OnceLock<MenuId> = OnceLock::new();
static SHOW_WINDOW_REQUESTED: AtomicBool = AtomicBool::new(false);
static QUIT_REQUESTED: AtomicBool = AtomicBool::new(false);

fn show_main_window() {
    SHOW_WINDOW_REQUESTED.store(true, Ordering::SeqCst);
}

pub struct TrayManager {
    tray_icon: TrayIcon,
    #[allow(dead_code)]
    menu: Menu,
    show_item_id: MenuId,
    quit_item_id: MenuId,
}

impl TrayManager {
    /// Creates new `TrayManager` with icon and context menu
    ///
    /// # Errors
    /// Returns error if icon file cannot be loaded, menu creation fails, or tray icon creation fails
    pub fn new() -> Result<Self> {
        let menu = Menu::new();

        let show_item = MenuItem::new("Pokaż okno", true, None);
        let quit_item = MenuItem::new("Zamknij", true, None);

        let show_item_id = show_item.id().clone();
        let quit_item_id = quit_item.id().clone();

        let _ = QUIT_ITEM_ID.set(quit_item_id.clone());
        let _ = SHOW_ITEM_ID.set(show_item_id.clone());

        MenuEvent::set_event_handler(Some(|event: MenuEvent| {
            if let Some(quit_id) = QUIT_ITEM_ID.get() {
                if event.id == *quit_id {
                    tracing::info!("Quit from tray requested");
                    QUIT_REQUESTED.store(true, Ordering::SeqCst);
                }
            }
            if let Some(show_id) = SHOW_ITEM_ID.get() {
                if event.id == *show_id {
                    show_main_window();
                }
            }
        }));

        TrayIconEvent::set_event_handler(Some(|event: TrayIconEvent| {
            if let TrayIconEvent::Click {
                button,
                button_state,
                ..
            } = event
            {
                use tray_icon::{MouseButton, MouseButtonState};
                if button == MouseButton::Left && button_state == MouseButtonState::Up {
                    show_main_window();
                }
            }
        }));

        menu.append(&show_item)
            .context("Failed to add 'Pokaż okno' to menu")?;
        menu.append(&quit_item)
            .context("Failed to add 'Zamknij' to menu")?;

        let icon = embedded_icons::load_icon(IconType::Normal)?;

        let tray_icon = TrayIconBuilder::new()
            .with_menu(Box::new(menu.clone()))
            .with_menu_on_left_click(false)
            .with_icon(icon)
            .with_tooltip("Drop2S3 - Przeciągnij pliki tutaj")
            .build()
            .context("Failed to build tray icon")?;

        Ok(Self {
            tray_icon,
            menu,
            show_item_id,
            quit_item_id,
        })
    }

    pub fn handle_tray_event(&self, event: &TrayIconEvent) -> bool {
        match event {
            TrayIconEvent::Click {
                button,
                button_state,
                ..
            } => {
                use tray_icon::MouseButton;
                use tray_icon::MouseButtonState;

                if *button == MouseButton::Left && *button_state == MouseButtonState::Up {
                    return true;
                }
                false
            }
            _ => false,
        }
    }

    /// Processes menu item events (menu clicks)
    ///
    /// Handles: "Pokaż okno", "Zamknij"
    pub fn handle_menu_event(&self, event: &MenuEvent) -> MenuAction {
        if event.id == self.show_item_id {
            tracing::info!("Menu: Pokaż okno clicked");
            MenuAction::ShowWindow
        } else if event.id == self.quit_item_id {
            tracing::info!("Menu: Zamknij clicked");
            MenuAction::Quit
        } else {
            tracing::warn!("Unknown menu event ID: {:?}", event.id);
            MenuAction::None
        }
    }

    pub fn should_show_window() -> bool {
        SHOW_WINDOW_REQUESTED.swap(false, Ordering::SeqCst)
    }

    pub fn quit_requested() -> bool {
        QUIT_REQUESTED.load(Ordering::SeqCst)
    }

    /// Polls menu events
    ///
    /// Returns Some(event) if event available, None otherwise
    pub fn poll_menu_event() -> Option<MenuEvent> {
        MenuEvent::receiver().try_recv().ok()
    }

    pub fn set_icon(&mut self, icon_type: IconType) -> Result<()> {
        let icon = embedded_icons::load_icon(icon_type)?;
        self.tray_icon
            .set_icon(Some(icon))
            .context("Failed to update tray icon")?;
        Ok(())
    }
}

/// Actions triggered by menu items
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuAction {
    ShowWindow,
    Quit,
    None,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_menu_action_variants() {
        let actions = [MenuAction::ShowWindow, MenuAction::Quit, MenuAction::None];

        for action in &actions {
            assert_eq!(*action, *action);
        }
    }

    #[test]
    fn test_menu_action_clone() {
        let action = MenuAction::Quit;
        let cloned = action.clone();
        assert_eq!(action, cloned);
    }

    #[test]
    fn test_menu_action_debug() {
        let action = MenuAction::ShowWindow;
        let debug_str = format!("{:?}", action);
        assert!(debug_str.contains("ShowWindow"));
    }
}
