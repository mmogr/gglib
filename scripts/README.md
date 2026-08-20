# GGLib Helper Scripts

This directory contains helper scripts for development, CI enforcement, and documentation generation.

## Quick Reference

| Script | Purpose | Used By |
|--------|---------|---------|
| [check_boundaries.sh](#check_boundariessh) | Validate crate dependency rules | CI |
| [check-frontend-ipc.sh](#check-frontend-ipcsh) | Enforce Tauri invoke() allowlist | CI |
| [check-tauri-commands.sh](#check-tauri-commandssh) | Enforce HTTP-first Tauri policy | CI |
| [check_file_complexity.sh](#check_file_complexitysh) | TypeScript/CSS file-size ratchet | CI |
| [check_rust_complexity.sh](#check_rust_complexitysh) | Rust file-size ratchet | CI |
| [check_param_source_exhaustive.sh](#check_param_source_exhaustivesh) | No catch-all arm over `ParamSource` | CI |
| [check_workflow_yaml.sh](#check_workflow_yamlsh) | No duplicate keys in workflow YAML | CI |
| [check_transport_branching.sh](#check_transport_branchingsh) | Enforce transport layer unification | CI |
| [check_settings_surfaces.sh](#check_settings_surfacessh) | Every `Settings` field is settable from somewhere | CI |
| [check-deps.sh](#check-depssh) | Verify system dependencies | `make check-deps` |
| [install-llama.sh](#install-llamash) | Install llama.cpp with GPU detection | `make llama-install-auto` |
| [generate_module_tables.sh](#generate_module_tablessh) | Update README badge tables | Manual |
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

### `check-frontend-ipc.sh`

Enforces Tauri `invoke()` allowlist in frontend code:
- Only OS integration commands should be invoked from frontend
- Prevents dynamic command string construction (security risk)

**Allowlist** (7 commands):
- `get_embedded_api_info` (API discovery)
- `check_llama_status` (binary management)
- `install_llama` (binary management)
- `open_url` (shell integration)
- `set_selected_model` (menu sync)
- `sync_menu_state` (menu sync)
- `log_from_frontend` (frontend log forwarding)

```bash
./scripts/check-frontend-ipc.sh
```

### `check-tauri-commands.sh`

Enforces "HTTP-first, OS-glue-only" Tauri command policy:
1. `#[tauri::command]` only in `{util,llama,app_logs}.rs`
2. No extra `.rs` files in `src-tauri/src/commands/`
3. No deprecated `get_gui_api_port` anywhere

```bash
./scripts/check-tauri-commands.sh
```

### `check_transport_branching.sh`

Three rules over `src/`:

1. no `isTauriApp` inside `src/services/clients/`;
2. a client module may import `transport/api/client` (the base-URL/auth
   primitive) and `transport/types/*` (declarations), but not a transport
   *domain* API — needing one means the module should not be a client;
3. remaining `isTauriApp` uses carry a `TRANSPORT_EXCEPTION:` comment (warning
   only).

Rule 2 self-tests against known-bad and known-good fixtures before the real scan,
and fails when zero client modules are scanned. Both guards exist because the
rule it replaced could never fail: it grepped for four identifiers that had never
existed as code, and an empty scan is indistinguishable from a clean one.

```bash
./scripts/check_transport_branching.sh
```

### `check_file_complexity.sh` / `check_rust_complexity.sh`

The file-size ratchets, one per language. A file already over the 300-LOC
budget is recorded in a baseline at its current size and may shrink freely;
growing it fails. A file not in the baseline may not cross the line at all.

A ratchet rather than a threshold because a threshold could not be switched on:
174 Rust files and 24 TypeScript ones are already over. A gate that fails on
every commit gets switched off within a day, which is how a constraint becomes
decorative — and `check_file_complexity.sh` *was* decorative, documented in
CONTRIBUTING and run by nothing at all.

`--update` rewrites the baseline. Use it when a file legitimately grew and the
growth is the point: the diff then shows the number going up.

```bash
./scripts/check_file_complexity.sh [--update]   # src/**/*.{ts,tsx,css}
./scripts/check_rust_complexity.sh [--update]   # crates/ and src-tauri/
```

### `check_param_source_exhaustive.sh`

Fails if anything matches on `ParamSource` with a catch-all arm — several
decisions read it to mean "did a person choose this?", and a wildcard makes
adding a variant a silent behaviour change instead of a compile error.

```bash
./scripts/check_param_source_exhaustive.sh
```

### `check_workflow_yaml.sh`

Fails on duplicate mapping keys in `.github/workflows/`. GitHub rejects such a
file outright — the run is marked "failed because of a workflow file issue" and
no jobs start, including the one that would have caught it.

```bash
./scripts/check_workflow_yaml.sh
```

### `check_settings_surfaces.sh`

Fails if a `Settings` field is settable from no surface a person has — no CLI
flag on `gglib config settings set`, and no camelCase mention anywhere in
`src/`. One surface is enough; several settings are deliberately CLI-only, and
`close_to_tray` means nothing to a terminal.

Written after `tool_call_repair` spent months stored, plumbed, read by the
proxy and settable from nowhere. Nothing catches that: every layer compiles,
and `config settings show` even printed it, because that display is derived
from serde. The failure is an absence, and absences do not fail type checks.

Exemptions live in the script with a reason each, and should stay rare — an
exemption claims the field is written by something other than a person.

```bash
./scripts/check_settings_surfaces.sh
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

Cargo crates use `version.workspace = true` so they inherit automatically.
`src-tauri/tauri.conf.json` declares no `version` key: Tauri falls back to the
`Cargo.toml` version when it is absent, so the app metadata inherits too.

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

| Workflow | Job | Scripts Used |
|----------|-----|--------------|
| `ci.yml` | `boundaries` | `check_boundaries.sh` |
| `ci.yml` | `enforcement` | `check-tauri-commands.sh`, `check-frontend-ipc.sh`, `check_transport_branching.sh`, `check_param_source_exhaustive.sh`, `check_settings_surfaces.sh`, `check_rust_complexity.sh`, `check_file_complexity.sh` |
| `ci.yml` | `quality` | `check_workflow_yaml.sh` |
| `check-issue-form.yml` | — | `check_issue_form_mapping.mjs` |
| `bump-version.yml` | — | `sync_versions.py` |
| `release.yml` | — | bundles `macos-install.command` |

`badges.yml` inlines its own badge generation and invokes no script here.
