# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build & Test Commands

```bash
# CRITICAL: Cannot build natively on Linux (missing pkg-config, openssl, gobject)
# Always use --target flag for cross-compilation from WSL2

cargo check --target x86_64-pc-windows-msvc
cargo build --target x86_64-pc-windows-msvc
cargo build --release --target x86_64-pc-windows-msvc
cargo test --target x86_64-pc-windows-msvc
cargo test --target x86_64-pc-windows-msvc -- test_name    # single test
cargo clippy --target x86_64-pc-windows-msvc -- -D warnings
```

CI runs on `windows-latest` with clippy + test + release build. Release triggered on `v*` tags.

## Architecture

**Portable Windows tray app** (1 exe + 1 config.toml) for drag-and-drop uploads to Oracle Cloud Object Storage via S3-compatible API.

### CLI Entry Points

- `drop2s3.exe` (no args) -- GUI app with system tray
- `drop2s3.exe --encrypt` -- credential encryption tool (reads from stdin)
- `drop2s3.exe --init-robots` -- generates robots.txt in the S3 bucket

### Event Loop (Lightweight/Heavy mode)

The main loop in `main.rs` alternates between two modes:
- **Lightweight mode**: Sleeps 200ms, pumps Windows messages, polls tray atomic flags (`QUIT_REQUESTED`, `SHOW_WINDOW_REQUESTED`)
- **Heavy mode**: `ui::show_window()` spawns egui window that blocks until closed, then returns to lightweight mode

### Data Flow: Upload

```
User drops files → ui.rs handle_dropped_files()
  → UploadManager::upload_files(Vec<PathBuf>)      [tokio async, buffer_unordered(3)]
    → upload_with_retry() per file                   [exponential backoff, max 3 retries]
      → S3Client::upload_file_auto_with_progress()   [small: PUT, large: multipart]
        → MultipartUploadGuard (RAII abort on drop)
  → UploadProgress sent via tokio::mpsc channel
    → UI polls with try_recv(), updates progress bar
  → On complete: History::add() + clipboard copy
```

### Key Module Interactions

- **main.rs** -- owns `AppState` (Arc), creates tokio runtime (2 threads), S3Client, UploadManager, TrayManager
- **tray.rs** -- atomic flags for lock-free cross-thread signaling (no mutex on hot path)
- **upload.rs** -- `S3Client` (wraps rust-s3 Bucket), `UploadManager` (parallel queue + cancel via `Mutex<CancellationToken>`)
- **ui.rs** -- egui drop zone, reads progress channel, manages clipboard, saves window position on close
- **config.rs** -- TOML loading/saving, `Config::Debug` redacts credentials as `[ENCRYPTED]`
- **portable_crypto.rs** -- XChaCha20-Poly1305 with embedded key (XOR-obfuscated). **Intentional design** for portable deployment
- **update.rs** -- GitHub API release check, download with size + optional SHA256 verification, apply on shutdown
- **shutdown_handler.rs** -- hidden HWND_MESSAGE window listens for WM_QUERYENDSESSION to trigger update-on-shutdown

### Where to Look

| Task | File(s) | Key types |
|------|---------|-----------|
| Upload logic / S3 | `upload.rs` | `S3Client`, `UploadManager`, `MultipartUploadGuard` |
| GUI / drag-drop / clipboard | `ui.rs` | egui window, drop zone, Ctrl+V paste |
| Configuration / TOML | `config.rs` | `Config`, validation, Debug redaction |
| Encryption | `portable_crypto.rs` | XChaCha20-Poly1305, embedded key |
| System tray | `tray.rs` | `TrayManager`, atomic flag signaling |
| Auto-update | `update.rs` | GitHub API, SHA256 verification |
| Upload history | `history.rs` | JSON persistence, FIFO ordering |
| Windows autostart | `startup.rs` | Registry `HKCU\...\Run` |
| Single instance guard | `single_instance.rs` | Global mutex |
| Shutdown handling | `shutdown_handler.rs` | `WM_QUERYENDSESSION` |
| Logging setup | `logging.rs` | tracing, daily rotation |
| CLI tools | `main.rs` | `--encrypt`, `--init-robots` flags |

### Concurrency Model

- Tokio multi-thread runtime (2 workers) for async S3 uploads
- `futures::stream::buffer_unordered(parallel_limit)` for concurrent file uploads
- `std::sync::Mutex<CancellationToken>` for cooperative cancel (not RwLock - prevents race)
- Atomic bools for tray<->main loop signaling
- History mutex released before disk I/O (deadlock prevention)

## Code Conventions

- **Error handling**: `anyhow::Result<T>` universally, `.context()`/`.with_context()` for rich errors, `bail!()` for early returns
- **Naming**: snake_case files/functions, PascalCase types, UPPER_CASE constants
- **Platform code**: `#[cfg(windows)]`/`#[cfg(not(windows))]` pairs with no-op stubs
- **RAII pattern**: `MultipartUploadGuard`, `SingleInstanceGuard` (cleanup in `Drop`)
- **State**: `Arc<AppState>` as single source, interior mutability with `Mutex`
- **Logging**: tracing crate, structured fields, daily rotation, info/debug/warn/error levels used strategically
- **Tests**: inline `#[cfg(test)] mod tests` in each module (53 tests total)

## Design Decisions

- **Embedded encryption key** in `portable_crypto.rs` is intentional - enables sharing config.toml across users without per-user encryption
- **No `dpapi_crypto.rs`** - removed legacy module (was Windows-user-tied, conflicts with portable model)
- **`OracleConfig` has no `access_key`/`secret_key`** - credentials only via `[credentials]` section (encrypted)
- **`#![allow(dead_code)]` removed** from crate level - individual `#[allow(dead_code)]` on intentionally unused struct fields only
- **Release profile**: `opt-level = "z"`, LTO, strip, panic=abort -> ~2-3 MB exe

## Anti-Patterns & Gotchas

- **NEVER** change `portable_crypto.rs` key constants -- breaks all existing encrypted configs
- **NEVER** use `RwLock` for `cancel_token` -- use `Mutex` (prevents race condition on cancel/reset)
- **NEVER** add `.await` in `Drop` impl -- use `std::thread::spawn` workaround as in `MultipartUploadGuard`
- **History mutex**: always release BEFORE disk I/O (deadlock prevention)
- **Unbounded progress channel**: acceptable for desktop app, NOT for servers
- **`buffer_unordered(3)`**: completion order is NOT guaranteed
- **egui window blocks main thread**: cannot run multiple windows simultaneously
- **`panic="abort"` in release**: no stack unwinding, destructors may not run

## Platform-Specific Code

All Windows-specific code uses `#[cfg(windows)]` / `#[cfg(not(windows))]` pairs with no-op stubs for Linux (enables `cargo check` cross-compilation). Key areas:
- `pump_windows_messages()` - Win32 message pump
- `single_instance.rs` - Global mutex
- `shutdown_handler.rs` - Message-only window
- `startup.rs` - Registry autostart (HKCU\...\Run)
- `ui.rs::get_screen_size()` - GetSystemMetrics vs fallback 1920x1080
