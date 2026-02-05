# CLAUDE.md — src/

See root `CLAUDE.md` for build commands, architecture overview, and design decisions.

## Module Map

| Module | Role | Key Exports |
|--------|------|-------------|
| `main.rs` | Entry point, `AppState` (Arc-shared), tokio runtime (2 threads), lightweight/heavy event loop | `AppState`, `run_app()` |
| `upload.rs` | `S3Client` (wraps rust-s3 `Bucket`), `UploadManager`, multipart with RAII `MultipartUploadGuard`, progress via `tokio::mpsc` | `S3Client`, `UploadManager`, `UploadProgress` |
| `ui.rs` | egui window: drag-drop zone, Ctrl+V paste, history list, progress bar, window position save/restore | `show_window()` |
| `config.rs` | TOML load/save, `Config` struct with custom `Debug` (redacts credentials as `[ENCRYPTED]`) | `Config`, `OracleConfig` |
| `update.rs` | GitHub Releases API polling, binary download with size check + optional SHA256, apply-on-shutdown | `UpdateManager` |
| `tray.rs` | System tray icon + context menu, atomic flags (`QUIT_REQUESTED`, `SHOW_WINDOW_REQUESTED`) | `TrayManager` |
| `history.rs` | JSON-persisted upload history, `Mutex`-guarded FIFO (max entries), mutex released before disk I/O | `History` |
| `portable_crypto.rs` | XChaCha20-Poly1305, embedded key XOR-obfuscated in binary | `encrypt()`, `decrypt()` |
| `shutdown_handler.rs` | Hidden `HWND_MESSAGE` window listening for `WM_QUERYENDSESSION` | `install_shutdown_handler()` |
| `single_instance.rs` | Global named mutex, RAII `SingleInstanceGuard` | `SingleInstanceGuard::try_acquire()` |
| `startup.rs` | Windows registry `HKCU\...\Run` read/write for autostart | `is_autostart_enabled()`, `set_autostart()` |
| `logging.rs` | `tracing_subscriber` with daily rotating file appender | `init_logging()` |
| `embedded_icons.rs` | Base64-embedded PNG/ICO icon assets, decode at runtime | `load_icon()` |
| `utils.rs` | `get_exe_dir()` — directory of the running executable | `get_exe_dir()` |

## Internal Dependencies

```
main.rs ──→ config, upload, ui, tray, history, logging, single_instance,
             shutdown_handler, startup, update, utils
upload.rs ──→ config (S3 credentials), portable_crypto (decrypt creds at use)
ui.rs ──→ upload (UploadManager), history, config, update, embedded_icons
config.rs ──→ portable_crypto (encrypt/decrypt credential fields on save/load)
tray.rs ──→ embedded_icons (tray icon asset)
update.rs ──→ utils (exe dir for binary replacement)
```

## Patterns Specific to This Directory

- **Platform stubs**: `#[cfg(windows)]` with `#[cfg(not(windows))]` no-op counterparts in `tray.rs`, `shutdown_handler.rs`, `single_instance.rs`, `startup.rs`, `ui.rs` — enables `cargo check` on Linux.
- **Polish filename transliteration**: `upload.rs` maps diacritics before upload (`ą→a`, `ć→c`, `ł→l`, `ń→n`, `ó→o`, `ś→s`, `ź→z`, `ż→z` and uppercase variants).
- **S3 object key format**: `YYYY-MM-DD/{sanitized_name}_{uuid16}.{ext}`.
- **Cancellation**: `Mutex<CancellationToken>` (not RwLock) — intentional to prevent race between cancel and reset.
- **Progress flow**: `UploadManager` sends `UploadProgress` events through unbounded `tokio::mpsc::channel`; `ui.rs` drains with `try_recv()` each frame.
- **RAII guards**: `MultipartUploadGuard` aborts incomplete multipart uploads on drop; `SingleInstanceGuard` releases global mutex on drop.
- **Deadlock prevention**: `history.rs` releases its `Mutex` before performing file I/O.

## Testing

All 53 tests live inline (`#[cfg(test)]` modules) within their respective source files. Most tests are in `upload.rs` (sanitization, path formatting) and `config.rs` (serialization, redaction). Run with:
```bash
cargo test --target x86_64-pc-windows-msvc
```
