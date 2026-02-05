# DROP2S3 — PROJECT KNOWLEDGE BASE

**Generated:** 2026-02-05 | **Commit:** db30d5b | **Branch:** master

## OVERVIEW

Windows system tray app for drag-and-drop file uploads to Oracle Cloud Object Storage. Rust 2021 + egui/eframe (GUI) + tokio (async) + rust-s3 (OCI S3-compatible API). Single portable `.exe` + `config.toml`.

## STRUCTURE

```
drop2s3-oracle/
├── src/
│   ├── main.rs              # Entry point, AppState, main loop (3 CLI modes)
│   ├── upload.rs            # S3Client + UploadManager + multipart (largest: 989 LOC)
│   ├── ui.rs                # egui DropZone window + event handling (640 LOC)
│   ├── config.rs            # TOML config parsing + validation
│   ├── tray.rs              # System tray icon + context menu
│   ├── history.rs           # Upload history (JSON persistence)
│   ├── portable_crypto.rs   # XChaCha20-Poly1305 credential encryption
│   ├── dpapi_crypto.rs      # Windows DPAPI encryption (legacy, unused at runtime)
│   ├── update.rs            # Auto-update from GitHub Releases
│   ├── startup.rs           # Windows registry auto-start (HKCU\Run)
│   ├── shutdown_handler.rs  # WM_ENDSESSION handler for graceful exit
│   ├── single_instance.rs   # Global mutex — one instance only
│   ├── embedded_icons.rs    # Procedurally generated tray icons (cloud shape)
│   ├── logging.rs           # tracing + daily file rotation
│   └── utils.rs             # get_exe_dir() helper
├── assets/
│   ├── icon.ico             # App icon (embedded via build.rs)
│   └── icon_uploading.ico   # Upload-state icon
├── build.rs                 # winres: embeds icon.ico into .exe
├── Cargo.toml               # Manifest — release profile: opt-level=z, LTO, strip
├── config.example.toml      # Template for end-users
└── specyfikacja.md          # Polish technical specification
```

## WHERE TO LOOK

| Task | Location | Notes |
|------|----------|-------|
| Upload logic / S3 | `src/upload.rs` | `S3Client`, `UploadManager`, multipart, retry w/ exponential backoff |
| GUI / drag-drop | `src/ui.rs` | `DropZoneApp` implements `eframe::App`; egui immediate-mode |
| Configuration | `src/config.rs` | `Config::load()` parses TOML; `Config::save()` persists window pos |
| Credential encryption | `src/portable_crypto.rs` | XChaCha20-Poly1305 with embedded XOR key |
| System tray | `src/tray.rs` | `TrayManager`; static AtomicBool for cross-thread signaling |
| Auto-update | `src/update.rs` | GitHub API → download .exe → swap on shutdown |
| CLI: encrypt | `src/main.rs:257` | `--encrypt` flag → stdin key pair → encrypted config block |
| CLI: robots.txt | `src/main.rs:297` | `--init-robots` flag → uploads robots.txt to bucket |
| Add new module | `src/main.rs:4-17` | Declare `mod xyz;` then create `src/xyz.rs` |

## CODE MAP

| Symbol | Type | Location | Role |
|--------|------|----------|------|
| `AppState` | struct | main.rs:27 | Central hub — Arc-shared across UI, tray, uploads |
| `S3Client` | struct | upload.rs:69 | Oracle OCI bucket handle (rust-s3 `Bucket`) |
| `UploadManager` | struct | upload.rs:358 | Parallel upload queue + cancellation + retry |
| `UploadProgress` | struct | upload.rs:348 | Channel message: file_id, bytes, status |
| `DropZoneApp` | struct | ui.rs:105 | egui App — renders drop zone, progress, history |
| `TrayManager` | struct | tray.rs:20 | System tray icon + menu; static event handlers |
| `Config` | struct | config.rs:9 | TOML schema: oracle, app, advanced, credentials |
| `History` | struct | history.rs:18 | Thread-safe JSON persistence (Mutex<HistoryInner>) |
| `UpdateManager` | struct | update.rs:20 | GitHub release checker + binary downloader |
| `EncryptedCredentials` | struct | portable_crypto.rs:36 | Serializable encrypted payload (version + base64 data) |
| `ShutdownHandler` | struct | shutdown_handler.rs:28 | Hidden HWND_MESSAGE window for WM_ENDSESSION |

## CONVENTIONS

- **Language**: Polish for UI strings, comments, commits. English for code identifiers.
- **Error handling**: `anyhow::Result` everywhere. `thiserror` defined but custom errors not yet used.
- **Logging**: `tracing` crate — structured fields preferred (`tracing::info!(key = %val, "msg")`).
- **Async boundary**: tokio runtime created in `main()`, `rt_handle` passed via `AppState`. UI thread is sync; spawns async via `rt_handle.spawn()`.
- **Cross-thread state**: `Arc<AppState>` with `std::sync::Mutex` (not tokio Mutex) for tray/config/progress. `AtomicBool` statics for quit/show signals.
- **Windows API**: `windows` crate v0.62, `#[cfg(windows)]` guards with `#[cfg(not(windows))]` stubs.
- **Release profile**: Aggressively optimized for size (`opt-level = "z"`, LTO, strip, panic=abort).
- **Cross-compilation**: Dev machine uses xwin toolchain (Linux → Windows MSVC). CI uses `windows-latest` runner.
- **Tests**: Unit tests co-located in modules (`#[cfg(test)]`). `tempfile` for filesystem tests. No integration tests dir.

## ANTI-PATTERNS (THIS PROJECT)

- **NEVER** add `unsafe` blocks without `#[cfg(windows)]` guard and corresponding `#[cfg(not(windows))]` stub.
- **NEVER** hold `Mutex` across async `.await` points or blocking I/O. Clone data first, release lock, then operate.
- **NEVER** use `std::process::exit()` — coordinate shutdown via `AtomicBool` flags + `CancellationToken`.
- **NEVER** load entire large files into memory for upload — use `tokio::fs::File` + `AsyncReadExt` streaming.
- **NEVER** use blocking `std::fs` operations in async tasks — use `tokio::fs` equivalents.
- **AVOID** unnecessary `.clone()` in upload progress callbacks — clone once, move into closure.
- **AVOID** `#[allow(dead_code)]` on production code — indicates unused features to clean up.
- `dpapi_crypto.rs` is **legacy** — credentials now use portable XChaCha20 encryption (version 2). DPAPI code remains for reference only.

## COMMANDS

```bash
# Build (cross-compile from Linux)
cargo build --target x86_64-pc-windows-msvc

# Build release (optimized, ~2-3 MB exe)
cargo build --release --target x86_64-pc-windows-msvc

# Test (requires Windows or GTK on Linux for tray-icon)
cargo test --target x86_64-pc-windows-msvc

# Lint (CI enforces -D warnings)
cargo clippy --target x86_64-pc-windows-msvc -- -D warnings

# Encrypt credentials (CLI mode)
./drop2s3.exe --encrypt

# Upload robots.txt (CLI mode)
./drop2s3.exe --init-robots
```

## NOTES

- **Single instance**: Uses Windows named mutex (`Global\Drop2S3_SingleInstance_Mutex`). Second launch shows MessageBox and exits.
- **URL format**: `{date}/{filename}_{uuid16}.{ext}` — UUID16 = first 16 hex chars of UUIDv4. Polish chars transliterated (`żółć` → `zolc`).
- **Update flow**: Check GitHub API on startup → download `drop2s3_new.exe` → swap binaries on app shutdown → cleanup `drop2s3_old.exe` on next start.
- **Window position**: Saved to `config.toml` on close, restored on next open. Falls back to bottom-right above taskbar if off-screen.
- **Multipart threshold**: Files ≥5MB use multipart upload (configurable). Below threshold: single PUT.
- **Icon generation**: No .ico files loaded at runtime — cloud icon is procedurally drawn in `embedded_icons.rs` (pixel art bitmap). Build.rs only embeds icon for Windows Explorer.
- **CI release**: Push tag `v*` → builds on Windows → creates GitHub Release with auto-generated Polish changelog.
- **Config security**: `config.toml` is gitignored. Credentials encrypted with XChaCha20-Poly1305 using embedded XOR-masked key. Not DPAPI — portable across Windows machines.

## TEST GAPS

- **No async tests**: upload.rs/update.rs have async code but no `#[tokio::test]`
- **No integration tests**: tests/ directory empty
- **Zero coverage**: ui.rs, main.rs, single_instance.rs
- **62 unit tests** in 11 modules using `tempfile::TempDir` for isolation
