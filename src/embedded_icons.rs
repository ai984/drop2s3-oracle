use anyhow::{Context, Result};
use tray_icon::Icon;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IconType {
    Normal,
    Uploading,
}

pub fn load_icon(icon_type: IconType) -> Result<Icon> {
    let size = 32_u32;
    let rgba = draw_cloud_icon(size, icon_type);

    Icon::from_rgba(rgba, size, size).context("Failed to create tray icon")
}

fn draw_cloud_icon(size: u32, icon_type: IconType) -> Vec<u8> {
    let mut pixels = vec![0u8; (size * size * 4) as usize];

    let (r, g, b) = match icon_type {
        IconType::Normal => (180, 210, 255),
        IconType::Uploading => (80, 220, 80),
    };

    let cloud: &[&[u8]] = &[
        b"     ########     ",
        b"   ############   ",
        b"  ##############  ",
        b" ################ ",
        b" ################ ",
        b"##################",
        b"##################",
        b"##################",
        b" ################ ",
    ];

    let ph = cloud.len();
    let pw = cloud[0].len();

    let scale = (size as f32 / pw as f32).min(size as f32 / ph as f32);
    let drawn_w = (pw as f32 * scale) as u32;
    let drawn_h = (ph as f32 * scale) as u32;
    let off_x = (size - drawn_w) / 2;
    let off_y = (size - drawn_h) / 2;

    for y in 0..size {
        for x in 0..size {
            if x >= off_x && x < off_x + drawn_w && y >= off_y && y < off_y + drawn_h {
                let px = ((x - off_x) as f32 / scale) as usize;
                let py = ((y - off_y) as f32 / scale) as usize;

                if py < ph && px < pw && cloud[py][px] == b'#' {
                    let idx = ((y * size + x) * 4) as usize;
                    pixels[idx] = r;
                    pixels[idx + 1] = g;
                    pixels[idx + 2] = b;
                    pixels[idx + 3] = 255;
                }
            }
        }
    }

    pixels
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
