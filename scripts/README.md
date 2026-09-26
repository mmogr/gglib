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
| [check_lint_inheritance.sh](#check_lint_inheritancesh) | Every crate inherits the workspace lints; allowed lints may not grow | CI |
| [check_param_source_exhaustive.sh](#check_param_source_exhaustivesh) | No catch-all arm over `ParamSource` | CI |
| [check_workflow_yaml.sh](#check_workflow_yamlsh) | Workflow sanity: duplicate YAML keys, and badges.yml module paths | CI |
| [check_transport_branching.sh](#check_transport_branchingsh) | Enforce transport layer unification | CI |
| [check_settings_surfaces.sh](#check_settings_surfacessh) | Every `Settings` field is settable from somewhere | CI |
| [check_swallowed_db_errors.sh](#check_swallowed_db_errorssh) | No `sqlx` query has its `Result` discarded | CI |
| [check_adrs.py](#check_adrspy) | Every link into `docs/adr/` resolves; every ADR reference is defined | CI |
| [check-deps.sh](#check-depssh) | Verify system dependencies | `make check-deps` |
| [install-llama.sh](#install-llamash) | Install llama.cpp with GPU detection | `make llama-install-auto` |
| [generate_submodule_readmes.sh](#generate_submodule_readmessh) | Create missing README stubs | Manual |
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

### `check_lint_inheritance.sh`

Two rules over the workspace members the root `Cargo.toml` lists:

1. a member's manifest has one lints table, `[lints]`, and its only key is
   `workspace = true`. Cargo will not combine that key with a crate's own lint
   keys, so a crate that declares any is held to none of the workspace's;
2. every lint named inside an `allow(...)` or `expect(...)` attribute in a
   member's `.rs` files is counted, `cfg_attr` forms included and whole-line
   `//` comments not, against `lint-allow-baseline.txt`. Each member has two
   numbers there, and neither may grow: every lint so named, and the lints in
   an attribute with no `reason = "…"`. The `.rs` files that no member
   directory holds, such as `crates/build_common.rs`, are counted in a row of
   their own, `(outside-members)`, and a failure there names them.

The files are the ones git lists in the working tree: tracked, or untracked
and not ignored. An ignored file, such as build output or a worktree nested in
the checkout, is not counted.

`--update` rewrites the baseline to the counts. It raises the first number but
refuses to raise the second. A count that failed, a baseline row listed twice
and a baseline row whose second or third field is not a number each fail the
check, and `--update` then writes nothing. The self-test runs before every
check: five manifests that must fail rule 1 and a member without one,
tried together and then the missing one and a bad one each alone; files whose
counts are known (in `src/`, `tests/`, a build script and outside every member,
and none in ignored, deleted or non-`.rs` files); baselines that each count
must fail against; an `--update` that must raise the first number; a counter
that exits 1 without printing a count, a row listed twice and a count field
that is not a number, the row listed twice also under an `--update` that must
leave the baseline as it was; a baseline row naming no member; and a tree in
which git lists no `.rs` file. Each fixture that must fail must also make the
check exit non-zero. Its fixture repository reads no global or system git
config. `--self-test` runs it alone.

```bash
./scripts/check_lint_inheritance.sh [--update | --self-test]
```

### `check_param_source_exhaustive.sh`

Fails if anything matches on `ParamSource` with a catch-all arm — several
decisions read it to mean "did a person choose this?", and a wildcard makes
adding a variant a silent behaviour change instead of a compile error.

```bash
./scripts/check_param_source_exhaustive.sh
```

### `check_workflow_yaml.sh`

Two checks over `.github/workflows/`:

1. no duplicate mapping keys;
2. every module and coverage path named in `badges.yml` still resolves to a
   directory or file under `crates/`.

The second lives here rather than in `badges.yml` because the two jobs that
extract test and coverage badges check out the `badges` branch, not the source,
so they cannot see `crates/`. This script runs under `make enforce`, where the
source is present.

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

### `check_swallowed_db_errors.sh`

Fails if a `sqlx` query's `Result` is discarded — `let _ = sqlx::query(…)`, or
`.ok();` on an awaited query.

`gglib-db/src/setup.rs` carried six of these, each under a comment reading
"Ignore error if column already exists". The comment names one error; the code
discards every error, so `no such table`, `database is locked` and `database or
disk is full` all read as success. #796 is what that cost: an `ALTER` placed
above the `CREATE` that makes its table failed silently, and every fresh install
ran without `benchmark_runs.applied_json` until a second boot.

Tolerating a *named* error is fine, and `setup.rs::is_unique_violation` is the
shape for it. What this bans is tolerating all of them by writing none of them
down.

A `let _` alongside `?`, `.unwrap()` or `.expect(` is cleared — that discards
the row, not the error. `row.try_get("col").ok()` in the row mappers is out of
scope by design and the script says so.

Self-tests against a known-bad and a known-good fixture before scanning, and
fails if it finds no queries at all.

```bash
./scripts/check_swallowed_db_errors.sh
```

### `check_adrs.py`

Fails if a link into `docs/adr/` does not resolve, or an ADR uses a reference
link it does not define. Over every file git tracks, it reads:

1. a relative link in a Markdown file that points into `docs/adr/`, or whose
   path has an `adr` segment, reference definitions and HTML `href`s
   included. The target must be a tracked file or a directory holding one,
   and a `#fragment` must name a heading's id or an HTML anchor;
2. a `https://github.com/mmogr/gglib/blob/main/docs/adr/…` URL in any file,
   the form doc comments use. URLs into other repositories are not read;
3. a bare `docs/adr/NNNN-name.md` or `docs/adr/log-NNNN.md` mention in any
   file;
4. inside `docs/adr/*.md`, logs included, every `[text][label]`, `[text][]`
   and `[label]`, which must have its definition in the same file. A block
   moved into a log without its definitions fails here.

A bracket in an ADR that is not a link is escaped, `\[like this\]`; one that
holds no letter, `#` or code span, such as the interval `[+0.1, +0.2]`, is
left alone. Links out of `docs/adr/`, and relative paths in files that are
not Markdown, are not checked.

Heading ids are computed by the script, not fetched from GitHub. A setext
heading is found only when its text is one line, after a blank line or an ATX
heading, that does not start with `#`, `>`, `|` or a list marker. GitHub
renders more setext headings than that, among them one whose text runs over
several lines or sits in a list item or blockquote, and a link to one of those
fails here. Each heading in the self-test's fixture gets the id GitHub gives
it, apart from the multi-line setext case; a heading written another way may
not. Emphasis is paired without CommonMark's rule of three, and across a
link's brackets, which GitHub does not do.

`--check` runs the self-test first, a fixture tree that plants faults for each
of the four checks above beside constructs that must not be read as links,
then scans, and fails if it finds no ADR or no link. `--self-test` runs the
fixtures alone.

```bash
./scripts/check_adrs.py --check
./scripts/check_adrs.py --self-test
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

### `generate_submodule_readmes.sh`

Creates missing README stubs: one wherever a README is missing in a
directory below a crate's `src/`, below `src-tauri/src/` or below the
TypeScript `src/` (with the `module-docs` markers), and in `tests/` or a
directory below it. A Rust stub takes its text from the `//!` block of the
directory's `mod.rs` when it has one. For each directory it stubs whose
`mod.rs` lacks `#![doc = include_str!("README.md")]`, the script adds that
line and puts a `// MIGRATION` comment above any `//!` block. It never
deletes a `//!` block; that is left to the author.

```bash
./scripts/generate_submodule_readmes.sh --create [--dry-run]
```

**Note**: Never modifies an existing README.

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
| `ci.yml` | `enforcement` | `check-tauri-commands.sh`, `check-frontend-ipc.sh`, `check_transport_branching.sh`, `check_param_source_exhaustive.sh`, `check_settings_surfaces.sh`, `check_swallowed_db_errors.sh`, `check_rust_complexity.sh`, `check_file_complexity.sh`, `check_lint_inheritance.sh`, `check_adrs.py` |
| `ci.yml` | `quality` | `check_workflow_yaml.sh` |
| `check-issue-form.yml` | — | `check_issue_form_mapping.mjs` |
| `bump-version.yml` | — | `sync_versions.py` |
| `release.yml` | — | bundles `macos-install.command` |

`badges.yml` inlines its own badge generation and invokes no script here.
