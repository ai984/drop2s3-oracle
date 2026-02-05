//! Windows system shutdown handler - receives WM_QUERYENDSESSION/WM_ENDSESSION
//! to apply pending updates before Windows terminates the process.

use std::sync::atomic::{AtomicBool, Ordering};

#[cfg(windows)]
use windows::core::PCWSTR;
#[cfg(windows)]
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
#[cfg(windows)]
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, RegisterClassW, HWND_MESSAGE, WINDOW_EX_STYLE, WM_ENDSESSION,
    WM_QUERYENDSESSION, WNDCLASSW, WS_OVERLAPPED,
};

static SYSTEM_SHUTDOWN_REQUESTED: AtomicBool = AtomicBool::new(false);

pub fn is_system_shutdown_requested() -> bool {
    SYSTEM_SHUTDOWN_REQUESTED.load(Ordering::SeqCst)
}

fn request_shutdown() {
    tracing::info!("System shutdown detected, requesting graceful exit");
    SYSTEM_SHUTDOWN_REQUESTED.store(true, Ordering::SeqCst);
}

#[cfg(windows)]
pub struct ShutdownHandler {
    _hwnd: HWND,
}

#[cfg(not(windows))]
pub struct ShutdownHandler;

#[cfg(windows)]
impl ShutdownHandler {
    /// # Safety
    /// Uses Windows API directly. The window is message-only (HWND_MESSAGE parent).
    pub fn new() -> Option<Self> {
        unsafe {
            let class_name = windows::core::w!("Drop2S3ShutdownHandler");

            let wc = WNDCLASSW {
                lpfnWndProc: Some(shutdown_window_proc),
                lpszClassName: class_name,
                ..Default::default()
            };

            let atom = RegisterClassW(&wc);
            if atom == 0 {
                tracing::warn!("Failed to register shutdown handler window class");
                return None;
            }

            let hwnd = CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                class_name,
                PCWSTR::null(),
                WS_OVERLAPPED,
                0,
                0,
                0,
                0,
                Some(HWND_MESSAGE),
                None,
                None,
                None,
            );

            match hwnd {
                Ok(hwnd) if !hwnd.is_invalid() => {
                    tracing::debug!("Shutdown handler window created");
                    Some(Self { _hwnd: hwnd })
                }
                _ => {
                    tracing::warn!("Failed to create shutdown handler window");
                    None
                }
            }
        }
    }
}

#[cfg(not(windows))]
impl ShutdownHandler {
    pub fn new() -> Option<Self> {
        Some(Self)
    }
}

#[cfg(windows)]
unsafe extern "system" fn shutdown_window_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_QUERYENDSESSION => {
            tracing::info!("WM_QUERYENDSESSION received - system is preparing to shut down");
            if let Err(e) = crate::update::UpdateManager::apply_update_on_shutdown() {
                tracing::warn!("Failed to apply update during QUERYENDSESSION: {}", e);
            }
            LRESULT(1)
        }
        WM_ENDSESSION => {
            if wparam.0 != 0 {
                tracing::info!("WM_ENDSESSION received - system is shutting down");
                request_shutdown();
                if let Err(e) = crate::update::UpdateManager::apply_update_on_shutdown() {
                    tracing::warn!("Failed to apply update during ENDSESSION: {}", e);
                }
            }
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shutdown_flag_default() {
        let _ = SYSTEM_SHUTDOWN_REQUESTED.compare_exchange(
            true,
            false,
            Ordering::SeqCst,
            Ordering::SeqCst,
        );
        assert!(!is_system_shutdown_requested());
    }

    #[test]
    fn test_request_shutdown() {
        SYSTEM_SHUTDOWN_REQUESTED.store(false, Ordering::SeqCst);
        request_shutdown();
        assert!(is_system_shutdown_requested());
        SYSTEM_SHUTDOWN_REQUESTED.store(false, Ordering::SeqCst);
    }
}
