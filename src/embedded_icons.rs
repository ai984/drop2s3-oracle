use anyhow::{Context, Result};
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
    
    let img = image::load_from_memory(ico_bytes)
        .context("Failed to decode icon")?;
    
    let rgba = img.resize_exact(64, 64, image::imageops::FilterType::Lanczos3).to_rgba8();
    let (width, height) = rgba.dimensions();
    
    Icon::from_rgba(rgba.into_raw(), width, height)
        .context("Failed to create tray icon from RGBA data")
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
