use anyhow::Result;

#[cfg(windows)]
use windows::Win32::{
    Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0},
    System::Threading::{CreateMutexW, ReleaseMutex, WaitForSingleObject},
};

const MUTEX_NAME: &str = "Global\\Drop2S3_SingleInstance_Mutex";

pub struct SingleInstanceGuard {
    #[cfg(windows)]
    handle: HANDLE,
}

impl SingleInstanceGuard {
    #[cfg(windows)]
    pub fn acquire() -> Result<Self> {
        use windows::core::PCWSTR;

        let mutex_name: Vec<u16> = MUTEX_NAME
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();

        unsafe {
            let handle = CreateMutexW(None, false, PCWSTR(mutex_name.as_ptr()))?;
            let wait_result = WaitForSingleObject(handle, 0);

            if wait_result == WAIT_OBJECT_0 {
                tracing::debug!("Single instance lock acquired");
                Ok(Self { handle })
            } else {
                let _ = CloseHandle(handle);
                anyhow::bail!("Another instance of Drop2S3 is already running")
            }
        }
    }

    #[cfg(not(windows))]
    pub fn acquire() -> Result<Self> {
        Ok(Self {})
    }
}

#[cfg(windows)]
impl Drop for SingleInstanceGuard {
    fn drop(&mut self) {
        unsafe {
            let _ = ReleaseMutex(self.handle);
            let _ = CloseHandle(self.handle);
        }
        tracing::debug!("Single instance lock released");
    }
}

#[cfg(not(windows))]
impl Drop for SingleInstanceGuard {
    fn drop(&mut self) {}
}

#[cfg(windows)]
pub fn show_already_running_message() {
    use windows::core::PCWSTR;
    use windows::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONINFORMATION, MB_OK};

    let title: Vec<u16> = "Drop2S3".encode_utf16().chain(std::iter::once(0)).collect();
    let message: Vec<u16> =
        "Drop2S3 jest już uruchomiony.\n\nSprawdź ikonę w zasobniku systemowym (obok zegara)."
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();

    unsafe {
        MessageBoxW(
            None,
            PCWSTR(message.as_ptr()),
            PCWSTR(title.as_ptr()),
            MB_OK | MB_ICONINFORMATION,
        );
    }
}

#[cfg(not(windows))]
pub fn show_already_running_message() {
    eprintln!("Drop2S3 is already running");
}
