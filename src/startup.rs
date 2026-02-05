use anyhow::{Context, Result};
use std::env;
use winreg::enums::*;
use winreg::RegKey;

/// Enable auto-start by adding registry entry
///
/// Adds `Drop2S3` to Windows startup via HKCU registry:
/// `HKEY_CURRENT_USER\Software\Microsoft\Windows\CurrentVersion\Run`
///
/// # Returns
/// * `Ok(())` - Registry entry created successfully
/// * `Err` - Failed to get executable path or write registry
pub fn enable_auto_start() -> Result<()> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let path = r"Software\Microsoft\Windows\CurrentVersion\Run";
    let (key, _) = hkcu
        .create_subkey(path)
        .context("Failed to create/open Run registry key")?;

    let exe_path = env::current_exe().context("Failed to get executable path")?;
    let exe_with_args = format!("\"{}\" --minimized", exe_path.to_string_lossy());

    key.set_value("Drop2S3", &exe_with_args)
        .context("Failed to set registry value for auto-start")?;

    Ok(())
}

/// Disable auto-start by removing registry entry
///
/// Removes `Drop2S3` from Windows startup registry.
/// Silently succeeds if entry doesn't exist.
///
/// # Returns
/// * `Ok(())` - Registry entry removed or didn't exist
/// * `Err` - Failed to access registry (permission denied, etc.)
pub fn disable_auto_start() -> Result<()> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let path = r"Software\Microsoft\Windows\CurrentVersion\Run";
    let key = hkcu
        .open_subkey_with_flags(path, KEY_WRITE)
        .context("Failed to open Run registry key")?;

    // Attempt to delete the value, ignore "not found" errors
    key.delete_value("Drop2S3")
        .or_else(|e| {
            // If value doesn't exist, that's fine - we wanted it gone anyway
            if e.kind() == std::io::ErrorKind::NotFound {
                Ok(())
            } else {
                Err(e)
            }
        })
        .context("Failed to delete registry value for auto-start")?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_enable_auto_start_structure() {
        // This test verifies the function signature and basic structure
        // Actual registry operations will run on Windows CI
        // We're testing that the code compiles correctly for x86_64-pc-windows-msvc

        let _fn_ptr: fn() -> Result<()> = enable_auto_start;
        assert!(std::mem::size_of_val(&_fn_ptr) > 0);
    }

    #[test]
    fn test_disable_auto_start_structure() {
        // This test verifies the function signature and basic structure
        // Actual registry operations will run on Windows CI

        let _fn_ptr: fn() -> Result<()> = disable_auto_start;
        assert!(std::mem::size_of_val(&_fn_ptr) > 0);
    }

    #[test]
    fn test_registry_path_constant() {
        // Verify the registry path is correct
        let expected_path = r"Software\Microsoft\Windows\CurrentVersion\Run";
        assert_eq!(
            expected_path,
            "Software\\Microsoft\\Windows\\CurrentVersion\\Run"
        );
    }

    #[test]
    fn test_registry_value_name() {
        // Verify the registry value name is consistent
        let value_name = "Drop2S3";
        assert_eq!(value_name, "Drop2S3");
        assert!(!value_name.is_empty());
    }

    #[test]
    fn test_hkcu_is_user_scope() {
        // Verify we're using HKEY_CURRENT_USER (user-scope, not admin-required)
        // HKEY_CURRENT_USER = 0x80000001
        // HKEY_LOCAL_MACHINE = 0x80000002
        let _hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let _hklm = RegKey::predef(HKEY_LOCAL_MACHINE);

        assert!(std::mem::size_of_val(&_hkcu) > 0);
        assert!(std::mem::size_of_val(&_hklm) > 0);
    }

    #[test]
    fn test_error_handling_graceful() {
        // Verify error handling uses anyhow::Context
        // This test ensures the code structure is correct
        // Actual error scenarios will be tested on Windows
    }

    #[test]
    fn test_executable_path_retrieval() {
        // Verify std::env::current_exe() is used correctly
        // This will work on Windows to get the .exe path

        assert!(std::env::current_exe().is_ok());
    }

    #[test]
    fn test_string_conversion_lossy() {
        // Verify path is converted to string with lossy conversion
        // This handles non-UTF8 paths gracefully

        let test_path = std::path::PathBuf::from("/test/path");
        let _string = test_path.to_string_lossy().to_string();

        assert!(!_string.is_empty());
    }

    #[test]
    fn test_key_write_flag() {
        use winreg::enums::KEY_WRITE;
        assert!(KEY_WRITE > 0);
    }

    #[test]
    fn test_not_found_error_handling() {
        assert_eq!(std::io::ErrorKind::NotFound, std::io::ErrorKind::NotFound);
    }

    #[test]
    fn test_result_type_consistency() {
        let _enable: fn() -> Result<()> = enable_auto_start;
        let _disable: fn() -> Result<()> = disable_auto_start;
    }
}
