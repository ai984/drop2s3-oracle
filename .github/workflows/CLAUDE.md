# CLAUDE.md — .github/workflows/

## Overview

CI/CD pipeline for Drop2s3Oracle. Single workflow file `ci.yml` handles building, testing, and releasing.

## Pipeline Structure

**Job 1: `build-and-test`** — runs on `windows-latest` for every push (main/master/dev), PR, and tag push.

| Step | What it does |
|------|-------------|
| Clippy | `cargo clippy --target x86_64-pc-windows-msvc -- -D warnings` — treats all warnings as errors |
| Tests | `cargo test --target x86_64-pc-windows-msvc` |
| Release build | `cargo build --release --target x86_64-pc-windows-msvc` |
| Artifact upload | `drop2s3.exe` with 30-day retention |

**Job 2: `release`** — runs **only** on `v*` tags, after `build-and-test` succeeds.

- Downloads the artifact from Job 1 (no rebuild)
- Generates SHA256 checksum via PowerShell
- Creates GitHub Release with `drop2s3.exe` + `drop2s3.exe.sha256`
- Release notes are auto-generated; body includes Polish installation instructions

## How to Trigger a Release

```bash
git tag v1.0.6
git push origin v1.0.6
```

Tag must match `v*` pattern. The release job requires `permissions.contents: write`.

## Environment

- `CARGO_TERM_COLOR=always`, `RUST_BACKTRACE=1` set globally
- Rust stable toolchain, target `x86_64-pc-windows-msvc`, clippy component
- Caching via `Swatinem/rust-cache@v2`

## Editing Guidelines

- Build commands are documented in root `CLAUDE.md` — keep in sync if targets or flags change
- Do not add Linux-native build steps; the project requires Windows MSVC target
- Manual runs supported via `workflow_dispatch` (no inputs defined)
