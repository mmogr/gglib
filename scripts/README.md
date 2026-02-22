# GGLib Helper Scripts

This directory contains helper scripts for development, CI enforcement, and documentation generation.

## Quick Reference

| Script | Purpose | Used By |
|--------|---------|---------|
| [check_boundaries.sh](#check_boundariessh) | Validate crate dependency rules | CI |
| [check-abstractions.sh](#check-abstractionssh) | Enforce database abstraction layer | CI |
| [check-frontend-ipc.sh](#check-frontend-ipcsh) | Enforce Tauri invoke() allowlist | CI |
| [check-tauri-commands.sh](#check-tauri-commandssh) | Enforce HTTP-first Tauri policy | CI |
| [check_file_complexity.sh](#check_file_complexitysh) | Flag large files for decomposition | Manual |
| [check_transport_branching.sh](#check_transport_branchingsh) | Enforce transport layer unification | CI |
| [check-deps.sh](#check-depssh) | Verify system dependencies | `make check-deps` |
| [install-llama.sh](#install-llamash) | Install llama.cpp with GPU detection | `make llama-install-auto` |
| [discover_modules.sh](#discover_modulessh) | Auto-discover crate modules | Badge generation |
| [generate_badges_for_crate.sh](#generate_badges_for_cratesh) | Generate badge JSONs for crate | CI (badges.yml) |
| [generate_module_tables.sh](#generate_module_tablessh) | Update README badge tables | CI |
| [generate_submodule_readmes.sh](#generate_submodule_readmessh) | Update submodule README templates | Manual |
| [complexity_hotspots.sh](#complexity_hotspotssh) | Find high-complexity files | Manual |
| [sync_versions.py](#sync_versionspy) | Sync version across package files | Release |
| [macos-install.command](#macos-installcommand) | macOS app installer | Release bundle |

---

## Architecture Enforcement Scripts

These scripts are run in CI to enforce architectural boundaries and prevent regression.

### `check_boundaries.sh`

Validates workspace crate dependency boundaries enforcing the layered architecture:
- **gglib-core**: Pure domain types, no adapter/infra deps
- **gglib-db**: Core + sqlx only, no adapter deps  
- **Adapters (cli, axum, tauri)**: Core + db + their local deps only

```bash
./scripts/check_boundaries.sh [--verbose]
```

**Output**: `boundary-status.json` with pass/fail per crate

**Exit codes**: 0 = pass, 1 = violation

### `check-abstractions.sh`

Prevents leaky database abstractions by checking:
1. `setup_database()` calls only in approved entry points
2. Raw SQL (`sqlx::query`) only in approved database modules

```bash
./scripts/check-abstractions.sh
```

### `check-frontend-ipc.sh`

Enforces Tauri `invoke()` allowlist in frontend code:
- Only OS integration commands should be invoked from frontend
- Prevents dynamic command string construction (security risk)

**Allowlist** (8 commands):
- `get_embedded_api_info` (API discovery)
- `check_llama_status` (binary management)
- `install_llama` (binary management)
- `open_url` (shell integration)
- `set_selected_model` (menu sync)
- `sync_menu_state` (menu sync)
- `set_proxy_state` (proxy OS integration)
- `log_from_frontend` (frontend log forwarding)

```bash
./scripts/check-frontend-ipc.sh
```

### `check-tauri-commands.sh`

Enforces "HTTP-first, OS-glue-only" Tauri command policy:
1. `#[tauri::command]` only in `{util,llama,app_logs,research_logs}.rs`
2. No extra `.rs` files in `src-tauri/src/commands/`
3. No deprecated `get_gui_api_port` anywhere

```bash
./scripts/check-tauri-commands.sh
```

### `check_transport_branching.sh`

Ensures platform-specific code (`isTauriApp`) never appears in client modules:
- Frontend transport should be unified (HTTP for both web and Tauri)
- Platform branching only allowed in designated platform layer files

```bash
./scripts/check_transport_branching.sh
```

---

## Development Utility Scripts

### `check-deps.sh`

Verifies that all necessary system dependencies are installed:
- `cargo` (Rust toolchain)
- `npm` (Node.js)
- `cmake` (for llama.cpp builds)
- `git`

```bash
./scripts/check-deps.sh
```

Used by `make check-deps`.

### `install-llama.sh`

Automated script to download, build, and install `llama.cpp`:
- Detects OS (macOS, Linux)
- Detects Hardware (Apple Silicon → Metal, NVIDIA GPU → CUDA)
- Configures CMake with appropriate acceleration flags
- Installs binaries to `.llama/bin/`

```bash
./scripts/install-llama.sh [--force]
```

Used by `make llama-install-auto`.

### `check_file_complexity.sh`

Flags large files that should be decomposed based on LOC threshold:

```bash
./scripts/check_file_complexity.sh [threshold_loc]
# Default threshold: 300 LOC
```

Checks TypeScript, TSX, and CSS files.

### `complexity_hotspots.sh`

Generates a ranked list of high-complexity files using `scc`:

```bash
./scripts/complexity_hotspots.sh [threshold]
# Default threshold: 40 complexity
```

Requires [scc](https://github.com/boyter/scc) (`brew install scc`).

---

## Documentation Generation Scripts

These scripts generate and maintain the badge tables and README documentation.

### `discover_modules.sh`

Auto-discovers modules in crate `src/` directories for badge generation:

```bash
./scripts/discover_modules.sh                         # All crates
./scripts/discover_modules.sh gglib-core              # Specific crate
./scripts/discover_modules.sh --modules-only gglib-core  # Module names only
./scripts/discover_modules.sh --format=json           # JSON output
```

**Output format**: `CRATE:MODULE:BADGE_PREFIX`

### `generate_badges_for_crate.sh`

Generates badge JSON files for all modules in a crate:

```bash
./scripts/generate_badges_for_crate.sh <crate-name> <metric> [options]
```

**Metrics**: `loc`, `complexity`, `coverage`, `tests`

**Options**:
- `--lcov-file <path>` — Path to lcov.info (for coverage)
- `--test-file <path>` — Path to test output (for tests)
- `--output-dir <path>` — Output directory (default: `./badges`)

Used by CI workflow `.github/workflows/badges.yml`.

### `generate_module_tables.sh`

Regenerates module badge tables in README files with `<!-- module-table:start/end -->` markers:

```bash
./scripts/generate_module_tables.sh           # Update all READMEs
./scripts/generate_module_tables.sh --check   # CI mode (exit 1 if outdated)
./scripts/generate_module_tables.sh --dry-run # Show changes without writing
```

### `generate_submodule_readmes.sh`

Updates existing README files with badge table templates:

```bash
./scripts/generate_submodule_readmes.sh [--dry-run]
```

**Note**: Never creates new README files — only updates existing ones.

---

## Release & Versioning Scripts

### `sync_versions.py`

Syncs version from workspace `Cargo.toml` to other package files:

```bash
python3 ./scripts/sync_versions.py
```

**Source of truth**: `[workspace.package] version` in root `Cargo.toml`

**Syncs to**:
- `package.json` (npm/frontend)
- `src-tauri/tauri.conf.json` (Tauri app metadata)

Cargo crates use `version.workspace = true` so they inherit automatically.

---

## macOS Release Scripts

### `macos-install.command`

Double-clickable installer for macOS release bundles:
- Removes quarantine attribute (`xattr -cr`)
- Optionally moves app to `/Applications`

Bundled with release tarballs for macOS.

### `MACOS-README.txt`

Plain text instructions for macOS users explaining:
- Why the installer is needed (unsigned app)
- How to run the installer (double-click or Terminal)
- What the installer does

---

## Usage in CI

The main CI workflows that use these scripts:

| Workflow | Scripts Used |
|----------|--------------|
| `ci.yml` | `check_boundaries.sh`, `check-tauri-commands.sh`, `check-frontend-ipc.sh`, `check_transport_branching.sh` |
| `badges.yml` | `generate_badges_for_crate.sh`, `discover_modules.sh` |
| `release.yml` | `sync_versions.py`, bundles `macos-install.command` |
