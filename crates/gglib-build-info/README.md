# gglib-build-info

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-build-info-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-build-info-complexity.json)

Shared build/version metadata for gglib frontends.

Every surface reports the same version and commit, so a bug report names a build
that can actually be checked out.

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
│  │      `SEMVER`       │  │      `GIT_SHA`      │  │    `HAS_GIT_SHA`    │         │
│  │      "0.2.9"        │  │   "a1b2c3d4e5f6"    │  │     true/false      │         │
│  └─────────────────────┘  └─────────────────────┘  └─────────────────────┘         │
│                                                                                     │
│  ┌─────────────────────┐  ┌─────────────────────┐  ┌─────────────────────┐         │
│  │   `LONG_VERSION`    │  │    `FINGERPRINT`    │  │      `SHA_LEN`      │         │
│  │ 0.2.9 (a1b2c3d4e5f6)│  │"a1b2c3d4e5f6-dirty" │  │         12          │         │
│  └─────────────────────┘  └─────────────────────┘  └─────────────────────┘         │
│                                                                                     │
└─────────────────────────────────────────────────────────────────────────────────────┘
```

## Exported Constants

| Constant | Type | Description | Example |
|----------|------|-------------|---------|
| `SEMVER` | `&str` | `SemVer` version from `Cargo.toml` | `"0.2.9"` |
| `SHA_LEN` | `usize` | How many hex characters of the commit id every surface shows | `12` |
| `GIT_SHA` | `&str` | Commit id, cut to `SHA_LEN`, or `"unknown"` | `"a1b2c3d4e5f6"` |
| `HAS_GIT_SHA` | `bool` | Whether `GIT_SHA` is a commit id rather than `"unknown"` | `true` |
| `LONG_VERSION` | `&str` | What every surface displays: version with commit if available | `"0.2.9 (a1b2c3d4e5f6)"` |
| `FINGERPRINT` | `&str` | Commit id plus a `-dirty` marker, for build-skew detection | `"a1b2c3d4e5f6-dirty"` |

### Why the width is fixed

`SHA_LEN` is applied in `build.rs`, not left to git. Git — and `gix`, through
`vergen-gix` — abbreviates a commit id from the size of the object database, so the
same commit yields a different prefix depending on how much history the clone holds.
That is how this crate once spent a release printing no commit at all: a hard-coded
seven-character check stopped matching the eight characters `gix` had begun emitting,
and `LONG_VERSION` silently fell back to a bare `SemVer`. Cutting a full commit id to a
constant width means two people building the same commit on different machines get the
same string.

## Usage

```rust
use gglib_build_info::{GIT_SHA, LONG_VERSION};

// CLI --version output
println!("gglib {LONG_VERSION}");
// Output: gglib 0.2.9 (a1b2c3d4e5f6)

// Check if running from a git checkout
if gglib_build_info::HAS_GIT_SHA {
    println!("Commit: {GIT_SHA}");
}
```

## Consumers

This crate is used by:
- **`gglib-cli`** — `--version` and `--help` output, and the daemon build-skew warning
- **`gglib-app`** (`src-tauri/`) — macOS About metadata
- **`gglib-axum`** — the `/health` body and the `/api/version` endpoint
- **`gglib-mcp`** and **`gglib-proxy`** — the version in MCP `clientInfo` / `serverInfo`

## Build Process

The `build.rs` script uses `vergen-gix` to extract git information at compile time:

1. Reads `CARGO_PKG_VERSION` for `SemVer`.
2. Asks `vergen-gix` for the **full** commit id and the dirty flag, then cuts the id to
   `SHA_LEN` itself — see "Why the width is fixed" above.
3. Replays `vergen-gix`'s own `rerun-if-changed` lines, which cover `.git/HEAD` *and* the
   resolved branch ref. Both are needed: a commit on the current branch rewrites the ref
   file and leaves `.git/HEAD` untouched, so watching `HEAD` alone bakes in a stale commit.
4. Emits `GGLIB_GIT_SHA` and `GGLIB_FINGERPRINT` as `cargo:rustc-env` directives.

`GGLIB_BUILD_SHA_SHORT` overrides the probe for packagers building outside a checkout. It
must carry at least `SHA_LEN` hex characters; anything else is refused with a
`cargo:warning` rather than silently ignored.

When git is unavailable (e.g. a downloaded tarball), the constants fall back to safe
defaults:
- `GIT_SHA` and `FINGERPRINT` → `"unknown"`
- `HAS_GIT_SHA` → `false`
- `LONG_VERSION` → just `SEMVER`

## Internal Structure

<details>
<summary><h2>Modules</h2></summary>

<!-- module-table:start -->
| Module | LOC | Complexity | Coverage |
|--------|-----|------------|----------|
<!-- module-table:end -->

</details>
