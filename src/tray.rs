use anyhow::{Context, Result};
use tray_icon::{
    menu::{Menu, MenuEvent, MenuItem, MenuId},
    Icon, TrayIcon, TrayIconBuilder, TrayIconEvent,
};

/// System tray manager for Drop2S3 application
/// Provides tray icon with context menu: "Pokaż okno", "Ustawienia", "Zamknij"
pub struct TrayManager {
    tray_icon: TrayIcon,
    #[allow(dead_code)]
    menu: Menu,
    show_item_id: MenuId,
    settings_item_id: MenuId,
    quit_item_id: MenuId,
}

impl TrayManager {
    /// Creates new TrayManager with icon and context menu
    ///
    /// # Errors
    /// Returns error if icon file cannot be loaded, menu creation fails, or tray icon creation fails
    pub fn new() -> Result<Self> {
        let menu = Menu::new();

        let show_item = MenuItem::new("Pokaż okno", true, None);
        let settings_item = MenuItem::new("Ustawienia", true, None);
        let quit_item = MenuItem::new("Zamknij", true, None);

        let show_item_id = show_item.id().clone();
        let settings_item_id = settings_item.id().clone();
        let quit_item_id = quit_item.id().clone();

        menu.append(&show_item)
            .context("Failed to add 'Pokaż okno' to menu")?;
        menu.append(&settings_item)
            .context("Failed to add 'Ustawienia' to menu")?;
        menu.append(&quit_item)
            .context("Failed to add 'Zamknij' to menu")?;

        let icon_path = crate::utils::get_exe_dir().join("assets/icon.ico");
        let icon = Self::load_icon(icon_path.to_str().unwrap_or("assets/icon.ico"))?;

        let tray_icon = TrayIconBuilder::new()
            .with_menu(Box::new(menu.clone()))
            .with_icon(icon)
            .with_tooltip("Drop2S3 - Przeciągnij pliki tutaj")
            .build()
            .context("Failed to build tray icon")?;

        Ok(Self {
            tray_icon,
            menu,
            show_item_id,
            settings_item_id,
            quit_item_id,
        })
    }

    fn load_icon(path: &str) -> Result<Icon> {
        let icon_bytes = std::fs::read(path)
            .with_context(|| format!("Failed to read icon file: {}", path))?;

        Icon::from_rgba(icon_bytes.clone(), 16, 16)
            .with_context(|| format!("Failed to parse icon from: {}", path))
    }

    /// Processes tray icon events (left click, right click)
    ///
    /// This is a placeholder - full event handling will be implemented in Task 14
    pub fn handle_tray_event(&self, event: &TrayIconEvent) {
        match event {
            TrayIconEvent::Click {
                button,
                button_state,
                ..
            } => {
                tracing::info!(
                    "Tray icon clicked: button={:?}, state={:?}",
                    button,
                    button_state
                );
                // TODO (Task 14): Implement window show logic
            }
            _ => {
                tracing::debug!("Unhandled tray event: {:?}", event);
            }
        }
    }

    /// Processes menu item events (menu clicks)
    ///
    /// Handles: "Pokaż okno", "Ustawienia", "Zamknij"
    pub fn handle_menu_event(&self, event: &MenuEvent) -> MenuAction {
        if event.id == self.show_item_id {
            tracing::info!("Menu: Pokaż okno clicked");
            MenuAction::ShowWindow
        } else if event.id == self.settings_item_id {
            tracing::info!("Menu: Ustawienia clicked");
            MenuAction::ShowSettings
        } else if event.id == self.quit_item_id {
            tracing::info!("Menu: Zamknij clicked");
            MenuAction::Quit
        } else {
            tracing::warn!("Unknown menu event ID: {:?}", event.id);
            MenuAction::None
        }
    }

    /// Polls tray icon events
    ///
    /// Returns Some(event) if event available, None otherwise
    pub fn poll_tray_event() -> Option<TrayIconEvent> {
        TrayIconEvent::receiver().try_recv().ok()
    }

    /// Polls menu events
    ///
    /// Returns Some(event) if event available, None otherwise
    pub fn poll_menu_event() -> Option<MenuEvent> {
        MenuEvent::receiver().try_recv().ok()
    }

    /// Sets the tray icon to a new icon file
    ///
    /// # Arguments
    /// * `icon_path` - Path to the icon file (e.g., "assets/icon_uploading.ico")
    ///
    /// # Errors
    /// Returns error if icon file cannot be loaded or icon update fails
    pub fn set_icon(&mut self, icon_path: &str) -> Result<()> {
        let icon = Self::load_icon(icon_path)?;
        self.tray_icon
            .set_icon(Some(icon))
            .context("Failed to update tray icon")?;
        Ok(())
    }
}

/// Actions triggered by menu items
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuAction {
    /// Show main window
    ShowWindow,
    /// Show settings dialog
    ShowSettings,
    /// Quit application
    Quit,
    /// No action
    None,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_menu_action_variants() {
        let actions = [
            MenuAction::ShowWindow,
            MenuAction::ShowSettings,
            MenuAction::Quit,
            MenuAction::None,
        ];

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
