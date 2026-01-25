# gglib-build-info

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-build-info-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-build-info-complexity.json)

Shared build/version metadata for gglib frontends.

## Overview

This crate provides compile-time constants for version information and git metadata, populated by its `build.rs` script using [vergen-gix](https://crates.io/crates/vergen-gix).

## Architecture

This is a **utility crate** — it has no layer dependencies and can be used by any crate in the workspace.

```text
┌─────────────────────────────────────────────────────────────────────────────────────┐
│                              gglib-build-info                                       │
│                    Compile-time version & git metadata                              │
├─────────────────────────────────────────────────────────────────────────────────────┤
│                                                                                     │
│  Constants (populated at compile time):                                             │
│  ┌─────────────────────┐  ┌─────────────────────┐  ┌─────────────────────┐         │
│  │      SEMVER         │  │   GIT_SHA_SHORT     │  │     GIT_DIRTY       │         │
│  │   "0.2.9"           │  │     "a1b2c3d"       │  │    true/false       │         │
│  └─────────────────────┘  └─────────────────────┘  └─────────────────────┘         │
│                                                                                     │
│  ┌─────────────────────┐  ┌─────────────────────┐  ┌─────────────────────┐         │
│  │    LONG_VERSION     │  │ LONG_VERSION_WITH_  │  │ ABOUT_SHORT_VERSION │         │
│  │ "0.2.9 (a1b2c3d)"   │  │       SHA           │  │     "a1b2c3d"       │         │
│  └─────────────────────┘  └─────────────────────┘  └─────────────────────┘         │
│                                                                                     │
└─────────────────────────────────────────────────────────────────────────────────────┘
```

## Exported Constants

| Constant | Type | Description | Example |
|----------|------|-------------|---------|
| `SEMVER` | `&str` | SemVer version from Cargo.toml | `"0.2.9"` |
| `GIT_SHA_SHORT` | `&str` | Short git commit hash (7 chars) | `"a1b2c3d"` |
| `GIT_DIRTY` | `bool` | Whether repo has uncommitted changes | `true` |
| `HAS_GIT_SHA` | `bool` | Whether git SHA is valid (not "unknown") | `true` |
| `LONG_VERSION` | `&str` | Version with SHA if available | `"0.2.9 (a1b2c3d)"` |
| `LONG_VERSION_WITH_SHA` | `&str` | Always includes SHA placeholder | `"0.2.9 (a1b2c3d)"` |
| `ABOUT_SHORT_VERSION` | `&str` | Short version for About dialogs | `"a1b2c3d"` |

## Usage

```rust
use gglib_build_info::{SEMVER, LONG_VERSION, GIT_SHA_SHORT};

// CLI --version output
println!("gglib {}", LONG_VERSION);
// Output: gglib 0.2.9 (a1b2c3d)

// Check if running from git repo
if gglib_build_info::HAS_GIT_SHA {
    println!("Commit: {}", GIT_SHA_SHORT);
}
```

## Consumers

This crate is used by:
- **gglib-cli** — For `--version` output and build info display
- **gglib-tauri** — For macOS About metadata and window titles
- **gglib-axum** — For `/api/version` endpoint

## Build Process

The `build.rs` script uses `vergen-gix` to extract git information at compile time:

1. Reads `CARGO_PKG_VERSION` for SemVer
2. Runs `git rev-parse --short HEAD` for commit SHA
3. Checks `git status --porcelain` for dirty state
4. Emits values as `cargo:rustc-env` directives

When git is unavailable (e.g., downloaded tarball), constants fall back to safe defaults:
- `GIT_SHA_SHORT` → `"unknown"`
- `GIT_DIRTY` → `false`
- `LONG_VERSION` → just `SEMVER`
