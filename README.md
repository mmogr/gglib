# GGLib - Rust GGUF Library Management Tool

[![CI](https://github.com/mmogr/gglib/actions/workflows/ci.yml/badge.svg)](https://github.com/mmogr/gglib/actions/workflows/ci.yml)
[![Coverage](https://github.com/mmogr/gglib/actions/workflows/coverage.yml/badge.svg)](https://github.com/mmogr/gglib/actions/workflows/coverage.yml)
[![Release](https://github.com/mmogr/gglib/actions/workflows/release.yml/badge.svg)](https://github.com/mmogr/gglib/actions/workflows/release.yml)
![Version](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/version.json)
[![License](https://img.shields.io/badge/license-GPLv3-blue)](LICENSE)

<!-- crate-docs:start -->

A multi-interface platform for managing and serving GGUF model files locally — catalog, serve, and chat via CLI, desktop GUI, web UI, or OpenAI-compatible proxy, all sharing one layered Rust backend.

## Architecture Overview

Cargo workspace with compile-time enforced boundaries. Adapters → infrastructure → core — never the reverse.

![Tests](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/tests.json)
![Coverage](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/coverage.json)
![Boundaries](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/boundary.json)

```text
┌─────────────────────────────────────────────────────────────────────────────────────┐
│                                    Core Layer                                       │
│                                                                                     │
│   ┌─────────────────────────────────────────────────────────────────────────────┐   │
│   │                              gglib-core                                     │   │
│   │              Pure domain types, ports & traits (no infra deps)              │   │
│   └─────────────────────────────────────────────────────────────────────────────┘   │
│                                                                                     │
└─────────────────────────────────────────────────────────────────────────────────────┘
                                          │
        ┌─────────────┬─────────────┬─────┴─────┬─────────────┬─────────────┐
        ▼             ▼             ▼           ▼             ▼             ▼
┌─────────────────────────────────────────────────────────────────────────────────────┐
│                              Infrastructure Layer                                   │
│                                                                                     │
│  ┌────────────┐ ┌────────────┐ ┌────────────┐ ┌────────────┐ ┌────────────┐       │
│  │  gglib-db  │ │ gglib-gguf │ │ gglib-mcp  │ │ gglib-proxy│ │gglib-voice │       │
│  │   SQLite   │ │ GGUF file  │ │    MCP     │ │  OpenAI-   │ │Voice mode  │       │
│  │   repos    │ │   parser   │ │  servers   │ │  compat    │ │STT/TTS/VAD │       │
│  └────────────┘ └────────────┘ └────────────┘ └────────────┘ └────────────┘       │
│                                                                                     │
│  ╔═══════════════════════════════════════════════════════════════════════════════╗  │
│  ║                          External Gateways                                    ║  │
│  ║                                                                               ║  │
│  ║  ┌────────────────────────────────────┐  ┌────────────────────────────────┐   ║  │
│  ║  │      gglib-runtime                 │  │      gglib-download            │   ║  │
│  ║  │  Process lifecycle manager         │  │  Download orchestrator         │   ║  │
│  ║  │  ONLY component that spawns        │  │  ONLY component that contacts  │   ║  │
│  ║  │  & manages llama-server            │  │  HuggingFace Hub               │   ║  │
│  ║  │                                    │  │  (via gglib-hf + optional      │   ║  │
│  ║  │                                    │  │   hf_xet subprocess)           │   ║  │
│  ║  └────────────────────────────────────┘  └────────────────────────────────┘   ║  │
│  ╚═══════════════════════════════════════════════════════════════════════════════╝  │
│                                                                                     │
└─────────────────────────────────────────────────────────────────────────────────────┘
                                          │
                                          ▼
┌─────────────────────────────────────────────────────────────────────────────────────┐
│                                   Facade Layer                                      │
│                                                                                     │
│   ┌─────────────────────────────────────────────────────────────────────────────┐   │
│   │                              gglib-gui                                      │   │
│   │         Shared GUI backend (ensures feature parity across adapters)         │   │
│   └─────────────────────────────────────────────────────────────────────────────┘   │
│                                                                                     │
└─────────────────────────────────────────────────────────────────────────────────────┘
                                          │
                      ┌───────────────────┼───────────────────┐
                      ▼                   ▼                   ▼
┌─────────────────────────────────────────────────────────────────────────────────────┐
│                                   Adapter Layer                                     │
│                                                                                     │
│   ┌─────────────────────┐  ┌──────────────────────┐  ┌──────────────────────────┐   │
│   │    gglib-cli        │  │    gglib-axum        │  │     gglib-tauri          │   │
│   │  CLI interface      │  │  HTTP server         │  │  Desktop application     │   │
│   │  (terminal UI)      │  │  ┌────────────────┐  │  │  ┌────────────────────┐  │   │
│   │                     │  │  │ Serves React   │  │  │  │ Embeds React UI    │  │   │
│   │                     │  │  │ UI (static)    │  │  │  │ (WebView assets)   │  │   │
│   │                     │  │  └────────────────┘  │  │  ├────────────────────┤  │   │
│   │                     │  │                      │  │  │ Embedded Axum      │  │   │
│   │                     │  │                      │  │  │ (HTTP endpoints)   │  │   │
│   │                     │  │                      │  │  └────────────────────┘  │   │
│   └─────────┬───────────┘  └──────────┬───────────┘  └───────────┬──────────────┘   │
│             │                         │                          │                  │
│             └─────────────────────────┼──────────────────────────┘                  │
│                                       │                                             │
│                  All adapters call infrastructure layer via:                        │
│                  • External Gateways (runtime, download)                            │
│                  • Other infrastructure services (db, gguf, mcp, proxy)             │
│                                       │                                             │
└───────────────────────────────────────┼─────────────────────────────────────────────┘
                                        │
                                        ▼
                            ╔═══════════════════════╗
                            ║  External Gateways    ║
                            ║  (from infra layer)   ║
                            ╚═══════════════════════╝
                                        │
                    ┌───────────────────┴────────────────────┐
                    ▼                                        ▼
          ┌──────────────────────┐              ┌──────────────────────┐
          │   gglib-runtime      │              │   gglib-download     │
          │   spawns/manages     │              │   calls HF Hub API   │
          └──────────┬───────────┘              └──────────┬───────────┘
                     │                                     │
                     ▼                                     ▼
┌─────────────────────────────────────────────────────────────────────────────────────┐
│                                  External Systems                                   │
│                                                                                     │
│               ┌──────────────────────────────┐                                      │
│               │   llama-server instances     │                                      │
│               │   (child processes)          │                                      │
│               └──────────────────────────────┘                                      │
│                                                                                     │
│               ┌──────────────────────────────┐                                      │
│               │   HuggingFace Hub API        │                                      │
│               │   (HTTPS endpoints)          │                                      │
│               └──────────────────────────────┘                                      │
│                                                                                     │
└─────────────────────────────────────────────────────────────────────────────────────┘
```

**Architecture Principles:**

- **Unified access**: All adapters call through infrastructure layer—never directly to external systems
- **External gateways**: Only `gglib-runtime` and `gglib-download` touch external systems
- **Tauri architecture**: Embeds both React UI assets AND Axum HTTP server internally
- **React UI as artifact**: Static files in Axum, bundled assets in Tauri, unused in CLI
- **Python hf_xet**: Internal subprocess within `gglib-download`, not an architectural boundary

**Frontend Architecture:**

- **Styling & UI Contracts**: See [src/styles/README.md](src/styles/README.md) for Tailwind-first architecture, design token system, platform parity requirements, and component migration roadmap

### Crate Metrics

#### Core Layer
| Crate | Tests | Coverage | LOC | Complexity |
|-------|-------|----------|-----|------------|
| [gglib-core](crates/gglib-core) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-tests.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-coverage.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-complexity.json) |

#### Infrastructure Layer
| Crate | Tests | Coverage | LOC | Complexity |
|-------|-------|----------|-----|------------|
| [gglib-db](crates/gglib-db) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-db-tests.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-db-coverage.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-db-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-db-complexity.json) |
| [gglib-gguf](crates/gglib-gguf) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-gguf-tests.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-gguf-coverage.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-gguf-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-gguf-complexity.json) |
| [gglib-hf](crates/gglib-hf) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-hf-tests.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-hf-coverage.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-hf-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-hf-complexity.json) |
| [gglib-download](crates/gglib-download) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-download-tests.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-download-coverage.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-download-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-download-complexity.json) |
| [gglib-mcp](crates/gglib-mcp) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-mcp-tests.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-mcp-coverage.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-mcp-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-mcp-complexity.json) |
| [gglib-proxy](crates/gglib-proxy) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-tests.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-coverage.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-complexity.json) |
| [gglib-runtime](crates/gglib-runtime) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-tests.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-coverage.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-complexity.json) |
| [gglib-voice](crates/gglib-voice) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-voice-tests.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-voice-coverage.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-voice-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-voice-complexity.json) |

#### Facade Layer
| Crate | Tests | Coverage | LOC | Complexity |
|-------|-------|----------|-----|------------|
| [gglib-gui](crates/gglib-gui) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-gui-tests.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-gui-coverage.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-gui-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-gui-complexity.json) |

#### Adapter Layer
| Crate | Tests | Coverage | LOC | Complexity |
|-------|-------|----------|-----|------------|
| [gglib-cli](crates/gglib-cli) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-tests.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-coverage.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-complexity.json) |
| [gglib-axum](crates/gglib-axum) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-tests.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-coverage.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-complexity.json) |
| [gglib-tauri](crates/gglib-tauri) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-tauri-tests.json) | ![N/A](https://img.shields.io/badge/coverage-N%2FA-lightgrey) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-tauri-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-tauri-complexity.json) |

#### Utility Crates
| Crate | Tests | Coverage | LOC | Complexity |
|-------|-------|----------|-----|------------|
| [gglib-build-info](crates/gglib-build-info) | ![N/A](https://img.shields.io/badge/tests-N%2FA-lightgrey) | ![N/A](https://img.shields.io/badge/coverage-N%2FA-lightgrey) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-build-info-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-build-info-complexity.json) |

### Crate Documentation

Each crate has its own README with architecture diagrams, module breakdowns, and design decisions:

| Layer | Crate | Description |
|-------|-------|-------------|
| **Core** | [gglib-core](crates/gglib-core/README.md) | Pure domain types, ports & traits |
| **Infra** | [gglib-db](crates/gglib-db/README.md) | SQLite repository implementations |
| **Infra** | [gglib-gguf](crates/gglib-gguf/README.md) | GGUF file format parser |
| **Infra** | [gglib-hf](crates/gglib-hf/README.md) | HuggingFace Hub client |
| **Infra** | [gglib-download](crates/gglib-download/README.md) | Download queue & manager |
| **Infra** | [gglib-mcp](crates/gglib-mcp/README.md) | MCP server management |
| **Infra** | [gglib-proxy](crates/gglib-proxy/README.md) | OpenAI-compatible proxy server |
| **Infra** | [gglib-runtime](crates/gglib-runtime/README.md) | Process manager & system probes |
| **Infra** | [gglib-voice](crates/gglib-voice/README.md) | Voice pipeline (STT/TTS/VAD) |
| **Facade** | [gglib-gui](crates/gglib-gui/README.md) | Shared GUI backend (feature parity) |
| **Adapter** | [gglib-cli](crates/gglib-cli/README.md) | CLI interface |
| **Adapter** | [gglib-axum](crates/gglib-axum/README.md) | HTTP API server |
| **Adapter** | [gglib-tauri](crates/gglib-tauri/README.md) | Desktop GUI (Tauri + React) |
| **Utility** | [gglib-build-info](crates/gglib-build-info/README.md) | Compile-time version & git metadata |

### Module Reference

#### Rust Crates
- **[`models`](src/models/README.md)** – GGUF model metadata, GUI/API DTOs, and data structures
- **[`services`](src/services/README.md)** – TypeScript client layer for GUI frontends
- **[`commands`](src/commands/README.md)** – CLI command handlers and web API endpoints
- **[`utils`](src/utils/README.md)** – Lower-level helpers for parsing and utilities

#### TypeScript Frontend
- **[`components`](src/components/README.md)** – React UI components
- **[`contexts`](src/contexts/README.md)** – React Context providers
- **[`hooks`](src/hooks/README.md)** – Custom React hooks
- **[`pages`](src/pages/README.md)** – Top-level page components
- **[`types`](src/types/README.md)** – Shared TypeScript type definitions

<!-- crate-docs:end -->

## Interfaces & Modes

All interfaces share the same backend (database, services, process manager, proxy). See each crate README for technical details.

| Interface | Launch | Details |
|-----------|--------|---------|
| **CLI** | `gglib <command>` | [gglib-cli](crates/gglib-cli/README.md) |
| **Desktop GUI** | `gglib gui` | [gglib-tauri](crates/gglib-tauri/README.md), [src-tauri](src-tauri/README.md) |
| **Web UI** | `gglib web` | [gglib-axum](crates/gglib-axum/README.md) — default `0.0.0.0:9887` |
| **OpenAI Proxy** | `gglib proxy` | [gglib-proxy](crates/gglib-proxy/README.md) — default `127.0.0.1:8080` |

<details>
<summary><strong>Security notes</strong></summary>

- Web server binds `0.0.0.0` (LAN-accessible); proxy binds `127.0.0.1` (local only) by default
- No authentication — designed for trusted networks
- Use firewall rules, private subnets, or VPN; do not expose to the public internet without additional auth

</details>

## Installation

### Pre-built Releases

Download from the [Releases page](https://github.com/mmogr/gglib/releases):

| Platform | Archive | Post-install |
|----------|---------|--------------|
| **macOS (Apple Silicon)** | `gglib-gui-*-aarch64-apple-darwin.tar.gz` | Run `macos-install.command` to remove quarantine |
| **macOS (Intel)** | `gglib-gui-*-x86_64-apple-darwin.tar.gz` | Same as above |
| **Linux** | `gglib-gui-*-x86_64-unknown-linux-gnu.tar.gz` | Run `gglib-gui` |
| **Windows** | `gglib-gui-*-x86_64-pc-windows-msvc.zip` | Run `gglib-gui.exe` |

### From Source

```bash
git clone https://github.com/mmogr/gglib.git && cd gglib
make setup   # check deps → build frontend → install CLI → offer llama.cpp install
```

`make setup` checks for Rust, Node.js, and build tools; provisions the Miniconda environment for the `hf_xet` fast download helper; builds the web UI; and installs the CLI to `~/.cargo/bin/`. It exits with an error if Python/Miniconda is missing — run it first on new machines.

> **Developer Mode:** When installed via `make setup`, the database (`gglib.db`), config (`.env`), and llama.cpp binaries live inside your repo folder. Downloaded models default to `~/.local/share/llama_models`.

### Prerequisites

- **Rust** 1.70+ — [rustup.rs](https://rustup.rs/)
- **Python 3 via Miniconda** — [miniconda](https://docs.conda.io/en/latest/miniconda.html) (for hf_xet fast downloads)
- **Node.js** 18+ — [nodejs.org](https://nodejs.org/) (for web UI)
- **SQLite** 3.x
- **Build tools**: macOS `xcode-select --install` + `brew install cmake` · Ubuntu `build-essential cmake git` · Windows VS 2022 C++ + CMake

llama.cpp is managed by GGLib — no separate install needed.

<details>
<summary><strong>Makefile targets</strong></summary>

**Installation & Setup:**
- `make setup` — Full setup (dependencies + build + install + llama.cpp)
- `make install` — Build and install CLI to `~/.cargo/bin/`
- `make uninstall` — Full cleanup (removes binary, system data, database; preserves models)

**Building:**
- `make build` / `make build-dev` — Release / debug binary
- `make build-gui` — Web UI frontend
- `make build-tauri` — Desktop GUI
- `make build-all` — Everything (CLI + web UI)

**Development:**
- `make test` / `make check` / `make fmt` / `make lint` / `make doc`

**llama.cpp:**
- `make llama-install-auto` / `make llama-status` / `make llama-update`

**Running:**
- `make run-gui` / `make run-web` / `make run-serve` / `make run-proxy`

**Cleaning:**
- `make clean` / `make clean-gui` / `make clean-llama` / `make clean-db`

</details>

<details>
<summary><strong>Manual installation (Cargo)</strong></summary>

```bash
cargo install --path .
```

</details>

<details>
<summary><strong>Configuring the models directory</strong></summary>

Default: `~/.local/share/llama_models`. Change via any of:

- `make setup` prompt
- `.env` file: `GGLIB_MODELS_DIR=/absolute/path`
- CLI: `gglib config models-dir set <path>` or `gglib --models-dir <path> download …`
- GUI/Web: Settings modal (gear icon)

Precedence: CLI flag → env var → default. Changing the directory does **not** move existing files.

</details>

## Development

Start the backend and frontend in separate terminals:

```bash
# Backend API server
cargo run --package gglib-cli -- web --api-only --port 9887 --base-port 9000

# Frontend dev server (proxies /api/* to backend)
npm run dev
# → http://localhost:5173
```

Or use the VS Code task **🚀 Run Dev (Frontend + Backend)** to launch both in parallel.

<details>
<summary><strong>Port configuration</strong></summary>

Set `VITE_GGLIB_WEB_PORT` in `.env` to change the API port (default `9887`). Both the Rust backend (via clap env) and Vite proxy read this value. The `VITE_` prefix is required for Vite. Port config only affects dev mode — production uses same-origin relative paths. Tauri uses dynamic port discovery.

</details>

<details>
<summary><strong>Production builds</strong></summary>

```bash
npm run build                                                      # → ./web_ui/
cargo run --package gglib-cli -- web --port 9887 --static-dir ./web_ui  # single-port serving
```

</details>

<details>
<summary><strong>Accelerated downloads (hf_xet)</strong></summary>

gglib bundles a managed Python helper for [hf_xet](https://github.com/huggingface/hf-xet) fast downloads. On first run (or after `make setup` / `gglib check-deps`), it provisions a Miniconda environment under `<data_root>/.conda/gglib-hf-xet` and installs `huggingface_hub>=1.1.5` + `hf_xet>=0.6`. There is no legacy Rust HTTP fallback — if the helper is missing, `gglib download` will fail until the environment is repaired.

</details>

<details>
<summary><strong>VS Code tasks</strong></summary>

- **🚀 Run Dev (Frontend + Backend)** — parallel launch
- **🧠 Run Backend Dev (API-only)** — backend only
- **🎨 Run Frontend Dev** — Vite dev server
- **🖥️ Run GUI (Dev)** — Tauri desktop in dev mode
- **🧪 Run All Tests** / **📎 Clippy** / **🎨 Format Code**

</details>

## Documentation

**[View Full API Documentation →](https://mmogr.github.io/gglib)**

Auto-generated from source and updated with every release.

## Acknowledgments

### Core & CLI
![Rust](https://img.shields.io/badge/rust-%23000000.svg?style=for-the-badge&logo=rust&logoColor=white)
![Clap](https://img.shields.io/badge/Clap-CLI-orange?style=flat-square)
![Tokio](https://img.shields.io/badge/Tokio-Async-blue?style=flat-square)
![SQLx](https://img.shields.io/badge/SQLx-Database-green?style=flat-square)
![Axum](https://img.shields.io/badge/Axum-Web_Server-darkred?style=flat-square)

### GUI & Frontend
![Tauri](https://img.shields.io/badge/tauri-%2324C8DB.svg?style=for-the-badge&logo=tauri&logoColor=white)
![React](https://img.shields.io/badge/react-%2320232a.svg?style=for-the-badge&logo=react&logoColor=%2361DAFB)
![Vite](https://img.shields.io/badge/vite-%23646CFF.svg?style=for-the-badge&logo=vite&logoColor=white)
![Tailwind CSS](https://img.shields.io/badge/tailwindcss-%2338B2AC.svg?style=for-the-badge&logo=tailwind-css&logoColor=white)
![Assistant UI](https://img.shields.io/badge/Assistant_UI-Chat_Interface-purple?style=flat-square)

### Integrations
![HuggingFace](https://img.shields.io/badge/%F0%9F%A4%97%20Hugging%20Face-Hub-yellow?style=flat-square)
![Llama.cpp](https://img.shields.io/badge/Llama.cpp-Inference-lightgrey?style=flat-square)
