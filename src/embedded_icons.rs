use anyhow::{Context, Result};
use image::imageops::FilterType;
use tray_icon::Icon;

const ICON_NORMAL: &[u8] = include_bytes!("../assets/icon.ico");
const ICON_UPLOADING: &[u8] = include_bytes!("../assets/icon_uploading.ico");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IconType {
    Normal,
    Uploading,
}

pub fn load_icon(icon_type: IconType) -> Result<Icon> {
    let ico_bytes = match icon_type {
        IconType::Normal => ICON_NORMAL,
        IconType::Uploading => ICON_UPLOADING,
    };

    let img = image::load_from_memory(ico_bytes).context("Failed to decode icon")?;

    let target_size = get_tray_icon_size();

    let resized = img.resize_exact(target_size, target_size, FilterType::Lanczos3);
    let rgba = resized.to_rgba8();

    Icon::from_rgba(rgba.into_raw(), target_size, target_size)
        .context("Failed to create tray icon from RGBA data")
}

#[cfg(target_os = "windows")]
fn get_tray_icon_size() -> u32 {
    use windows::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CXSMICON};
    let size = unsafe { GetSystemMetrics(SM_CXSMICON) };
    if size > 0 {
        size as u32
    } else {
        32
    }
}

#[cfg(not(target_os = "windows"))]
fn get_tray_icon_size() -> u32 {
    32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_embedded_icons_not_empty() {
        assert!(!ICON_NORMAL.is_empty());
        assert!(!ICON_UPLOADING.is_empty());
    }

    #[test]
    fn test_icon_type_variants() {
        assert_ne!(IconType::Normal, IconType::Uploading);
    }
}
