# GGLib - Rust GGUF Library Management Tool

[![CI](https://github.com/mmogr/gglib/actions/workflows/ci.yml/badge.svg)](https://github.com/mmogr/gglib/actions/workflows/ci.yml)
[![Coverage](https://github.com/mmogr/gglib/actions/workflows/coverage.yml/badge.svg)](https://github.com/mmogr/gglib/actions/workflows/coverage.yml)
[![Release](https://github.com/mmogr/gglib/actions/workflows/release.yml/badge.svg)](https://github.com/mmogr/gglib/actions/workflows/release.yml)
![Version](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/version.json)
[![License](https://img.shields.io/badge/license-GPLv3-blue)](LICENSE)

<!-- crate-docs:start -->

A multi-interface platform for managing and serving GGUF (GPT-Generated Unified Format) model files, providing CLI, desktop GUI, and web-based access.

## Overview

gglib provides a simple interface to catalog, organize, and serve GGUF models locally. It maintains a SQLite database of your models with their metadata, making it easy to find and work with specific models.

## Features

- **Add models**: Import GGUF files and extract metadata automatically
- **List models**: View all models with their properties in a clean table format
- **Update models**: Edit model metadata including name, parameters, and custom fields
- **Remove models**: Clean removal of models from your database
- **Serve models**: Start llama-server with automatic context size detection
- **Chat via CLI**: Launch llama-cli directly for quick terminal chat sessions
- **Question command**: Ask one-shot questions with optional piped context (`cat file | gglib question "summarize"`)
- **OpenAI-compatible Proxy**: Automatic model swapping with OpenAI API compatibility
- **HuggingFace Hub Integration**: Download models directly from HuggingFace Hub
- **Fast-path Downloads**: Managed Python helper (hf_xet) via Miniconda for multi-gigabyte transfers
- **Search & Browse**: Discover GGUF models on HuggingFace with search and browse commands
- **Quantization Support**: Intelligent detection and handling of various quantization formats
- **Rich metadata**: Support for complex metadata operations and Unicode content
- **Model Verification**: SHA256 integrity checking against HuggingFace LFS OIDs with per-shard progress, update detection, and automatic repair of corrupt files
- **Reasoning Model Support**: Auto-detection and streaming of thinking/reasoning phases with collapsible UI for models like DeepSeek R1, Qwen3, and QwQ

## Architecture Overview

GGLib is organized as a Cargo workspace with compile-time enforced boundaries. The architecture follows a layered design where adapters depend on infrastructure, which depends on core—never the reverse.

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

<details>
<summary><strong>Crate Metrics</strong></summary>

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

</details>

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

### Crate Documentation

Each crate has its own README with architecture diagrams, module breakdowns, and usage examples:

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
| **Facade** | [gglib-gui](crates/gglib-gui/README.md) | Shared GUI backend (feature parity) |
| **Adapter** | [gglib-cli](crates/gglib-cli/README.md) | CLI interface |
| **Adapter** | [gglib-axum](crates/gglib-axum/README.md) | HTTP API server |
| **Adapter** | [gglib-tauri](crates/gglib-tauri/README.md) | Desktop GUI (Tauri + React) |
| **Utility** | [gglib-build-info](crates/gglib-build-info/README.md) | Compile-time version & git metadata |

<!-- crate-docs:end -->

## Installation

GGLib provides a streamlined installation process using the included Makefile for the best developer and user experience.

### Pre-built Releases

Download the latest release for your platform from the [Releases page](https://github.com/mmogr/gglib/releases).

#### macOS

1. Download the macOS release tarball for your architecture:
   - `gglib-gui-*-aarch64-apple-darwin.tar.gz` for Apple Silicon (M1/M2/M3)
   - `gglib-gui-*-x86_64-apple-darwin.tar.gz` for Intel Macs
2. Extract the archive: `tar -xzf gglib-gui-*.tar.gz`
3. Double-click `macos-install.command` (or run `./macos-install.command` in Terminal)
4. The script will remove the quarantine attribute and optionally install to `/Applications`

> **Note:** macOS marks downloaded apps as "damaged" because they are not code-signed. The install script fixes this automatically by removing the quarantine attribute.

#### Windows

Download and extract the Windows release (`gglib-gui-*-x86_64-pc-windows-msvc.zip`), then run `gglib-gui.exe`.

#### Linux

Download and extract the Linux release (`gglib-gui-*-x86_64-unknown-linux-gnu.tar.gz`), then run the `gglib-gui` binary.

### Quick Install (From Source)

The recommended way to install GGLib is using the Makefile:

```bash
# Clone the repository
git clone https://github.com/mmogr/gglib.git
cd gglib

# Full setup: check dependencies, build, and install
make setup
```

The `make setup` command will:
- Check for required system dependencies (Rust, Node.js, build tools)
- Provision the managed Miniconda environment used by the hf_xet fast download helper
- Build the web UI frontend
- Build and install the CLI tool to `~/.cargo/bin/`
- Optionally install llama.cpp with automatic GPU detection

`make setup` (and `gglib check-deps`) exits with an error if Python/Miniconda is missing or the fast download helper cannot be prepared. Run those commands first on new machines so large downloads succeed without manual intervention.

**Note:** When installed via `make setup`, GGLib operates in **Developer Mode**. It will keep its database (`gglib.db`), configuration (`.env`), and `llama.cpp` binaries inside your repository folder. This keeps your development environment self-contained. (Downloaded models are still stored in `~/.local/share/llama_models` by default).

### Makefile Utilities

The Makefile provides several convenient targets:

**Installation & Setup:**
- `make setup` - Full setup (dependencies + build + install + llama.cpp)
- `make install` - Build and install CLI to `~/.cargo/bin/`
- `make uninstall` - **Full Cleanup**: Removes binary, system data, database, and cleans the repo. (Preserves downloaded models).

**Building:**
- `make build` - Build release binary
- `make build-dev` - Build debug binary
- `make build-gui` - Build web UI frontend
- `make build-tauri` - Build desktop GUI application
- `make build-all` - Build everything (CLI + web UI)

**Development:**
- `make test` - Run all tests
- `make check` - Check code without building
- `make fmt` - Format code
- `make lint` - Run clippy linter
- `make doc` - Generate and open documentation

**llama.cpp Management:**
- `make llama-install-auto` - Install llama.cpp with auto GPU detection
- `make llama-status` - Show llama.cpp installation status
- `make llama-update` - Update llama.cpp to latest version

**Running:**
- `make run-gui` - Launch desktop GUI
- `make run-web` - Start web server
- `make run-serve` - Run model server
- `make run-proxy` - Run OpenAI-compatible proxy

**Cleaning:**
- `make clean` - Remove build artifacts
- `make clean-gui` - Remove web UI build
- `make clean-llama` - Remove llama.cpp installation
- `make clean-db` - Remove database files

### Manual Installation (Alternative)

If you prefer to use Cargo directly:

```bash
# Install from source
cargo install --path .

# Or install from crates.io (when published)
cargo install gglib
```

### Prerequisites

- **Rust** 1.70 or later - [Install Rust](https://rustup.rs/)
- **Python 3 via Miniconda** (required for the hf_xet fast download helper) - [Install Miniconda](https://docs.conda.io/en/latest/miniconda.html)
- **Node.js** 18+ (for web UI) - [Install Node.js](https://nodejs.org/)
- **SQLite** 3.x
- **Build tools** (platform-specific):
  - **macOS**: `xcode-select --install` and `brew install cmake`
  - **Ubuntu/Debian**: `sudo apt install build-essential cmake git`
  - **Fedora/RHEL**: `sudo dnf install gcc-c++ cmake git`
  - **Arch Linux**: `sudo pacman -S base-devel cmake git`
  - **Windows**: Visual Studio 2022 with C++ tools, CMake, and Git

**Note:** llama.cpp is managed by GGLib itself. You don't need to install it separately!

### Configuring the models directory

Downloaded GGUF files now live in a user-configurable directory (default: `~/.local/share/llama_models`). You can change it at any time using whichever interface is most convenient:

- **During `make setup`** – the installer now prompts for the location and accepts Enter to keep the default.
- **Environment file** – copy `.env.example` to `.env` and set `GGLIB_MODELS_DIR=/absolute/path`, or edit the value via `gglib config models-dir set` (see below). All helpers expand `~/` and will create the directory when needed.
- **CLI overrides** – use `gglib --models-dir /tmp/models download …` for a one-off run, or persist the change with `gglib config models-dir prompt|set <path>`.
- **GUI/Web settings** – click the gear icon in the header to open the Settings modal, review the current directory, and update it without touching the CLI.

The precedence order is: CLI `--models-dir` flag → `GGLIB_MODELS_DIR` from the environment/.env → default path. All download code paths rely on the shared helper in `src/utils/paths.rs`, so whichever option you choose applies consistently across CLI, desktop, web, and background tasks.

## Development Setup

### Running the Development Environment

GGLib uses a split development setup with separate processes for frontend and backend:

1. **Start the Backend API Server:**
   ```bash
   # Using VS Code task (recommended):
   # Press Cmd/Ctrl+Shift+P → "Tasks: Run Task" → "🧠 Run Backend Dev (API-only)"
   
   # Or manually:
   cargo run --package gglib-cli -- web --api-only --port 9887 --base-port 9000
   ```

2. **Start the Frontend Dev Server:**
   ```bash
   npm run dev
   # Vite will start on port 5173 and proxy API requests to port 9887
   ```

3. **Access the application:**
   - Open your browser to `http://localhost:5173`
   - The frontend proxies all `/api/*` requests to the backend on port 9887

### Configuring Development Ports

You can customize the API port using the `VITE_GGLIB_WEB_PORT` environment variable. Both the backend and frontend will automatically use this value:

```bash
# Create a .env file in the project root
echo "VITE_GGLIB_WEB_PORT=9999" > .env

# Now start both servers - they'll both use port 9999
cargo run --package gglib-cli -- web --api-only
npm run dev
```

**How it works:**
- The Rust backend reads `VITE_GGLIB_WEB_PORT` via clap's environment support
- Vite's proxy configuration reads the same variable via `process.env.VITE_GGLIB_WEB_PORT`
- The frontend client code reads it via `import.meta.env.VITE_GGLIB_WEB_PORT`
- Default port is `9887` if the variable is not set

**Note:** 
- The `VITE_` prefix is required for Vite to expose the variable to the frontend
- Port configuration only affects **dev mode** - production builds use same-origin relative paths
- Tauri (desktop) mode uses dynamic port discovery and ignores this variable

### VS Code Tasks

The repository includes pre-configured VS Code tasks for common development workflows:

- **🚀 Run Dev (Frontend + Backend)** - Starts both frontend and backend in parallel
- **🧠 Run Backend Dev (API-only)** - Backend server for web development
- **🎨 Run Frontend Dev** - Vite dev server only
- **🖥️ Run GUI (Dev)** - Launch Tauri desktop app in development mode
- **🧪 Run All Tests** - Execute the full test suite
- **📎 Clippy (Linter)** - Run Rust linter
- **🎨 Format Code** - Auto-format Rust and TypeScript code

### Production Builds

The production build workflow differs from development:

1. **Build Frontend:**
   ```bash
   npm run build
   # Output: ./web_ui/ directory with static assets
   ```

2. **Run Backend with Static Serving:**
   ```bash
   cargo run --package gglib-cli -- web --port 9887 --static-dir ./web_ui
   # Backend serves both API and static frontend on single port
   ```

In production mode, the frontend uses relative URLs (no hardcoded ports) and communicates with the backend on the same origin.

Changing the directory only affects future downloads and servers—it does **not** move any GGUF files you already downloaded. If you want your existing models in the new location, move them manually and then rescan/add them as needed.

### Accelerated downloads via hf_xet

Large GGUFs can saturate a single HTTP stream, so gglib bundles a managed Python helper that talks to Hugging Face's [hf_xet](https://github.com/huggingface/hf-xet) service. Fast downloads are now the only path—if the helper is missing or broken, commands like `gglib download` will fail until you repair the environment.

- On the first run (or after `gglib check-deps`/`make setup`), gglib provisions a Miniconda environment under `<data_root>/.conda/gglib-hf-xet` and installs `huggingface_hub>=1.1.5` plus `hf_xet>=0.6`. A tiny helper script lives in `<data_root>/.gglib-runtime/python/hf_xet_downloader.py`.
- The helper emits newline-delimited JSON so both the CLI and GUI can keep their existing progress indicators.
- Missing Python packages are treated as errors. Run `gglib check-deps` or `make setup` to reinstall the managed environment; there is no legacy Rust HTTP fallback anymore.

Requirements: install Miniconda (or another Python 3 distribution with `venv` support) and ensure enough disk space to populate the per-user `.conda/gglib-hf-xet` directory. The helper respects the same Hugging Face tokens you pass to `gglib download` and does not change how downloads are recorded in the SQLite database.

## Interfaces & Modes

GGLib provides three complementary interfaces for interacting with GGUF models. All interfaces share the same backend implementation (database, services, process manager, and proxy), ensuring consistent behavior and data across all modes.

### CLI (Command-Line Interface)

Command-line interface for GGUF model management and service control.

**Capabilities:**
- Model operations: `gglib add`, `gglib list`, `gglib remove`, `gglib update`
- HuggingFace Hub integration: `gglib download`, `gglib search`, `gglib browse`
- Direct terminal chat: `gglib chat <id|name>`
- Server management: `gglib serve`, `gglib proxy`
- Interface launchers: `gglib gui`, `gglib web`
- llama.cpp management: `gglib llama install`, `gglib llama update`

### Desktop GUI (Tauri)

Cross-platform desktop application built with Tauri (Rust backend) and React frontend.

**Technical details:**
- Launched via `gglib gui` command
- Uses shared `GuiBackend` service for all operations
- Spawns embedded HTTP API server on localhost for frontend-backend communication
- React frontend communicates via standard HTTP endpoints (`/api/models`, `/api/servers`, etc.)
- Same API routes as standalone web server
- Shares business logic, data model, and process management with other interfaces

### Web UI + HTTP API

Browser-based interface backed by Axum HTTP server.

**Technical details:**
- Started via `gglib web` command
- Default binding: `0.0.0.0:9887` (LAN accessible)
- API routes: `/api/models`, `/api/servers`, `/api/chat`, `/api/proxy/...`
- React frontend (in `src/`) uses same HTTP endpoints as Tauri embedded server
- Services layer (`TauriService`, `ChatService`) detects environment and uses either Tauri IPC (`invoke`) or HTTP calls

## Web UI Server

The web server provides browser-based access to GGLib's functionality via an Axum HTTP API.

**Configuration:**
```bash
# Start web server (binds to 0.0.0.0:9887 by default)
gglib web --port 9887 --base-port 9000
```

**Access:**
- Local: `http://localhost:9887/`
- Network: `http://<HOST_IP>:9887/`
- API: `http://<HOST_IP>:9887/api`

**Parameters:**
- `--port`: Web server port (default: 9887)
- `--base-port`: Starting port for llama-server instances (default: 9000)

## OpenAI-Compatible Proxy

The proxy provides OpenAI API-compatible endpoints for model inference. This enables GGLib to work seamlessly with OpenWebUI and other tools that support the OpenAI API format.

**Configuration:**
```bash
# Start proxy (binds to 127.0.0.1:8080 by default)
gglib proxy --host 0.0.0.0 --port 8080 --llama-port 5500
```

**Endpoints:**
- Base URL: `http://<HOST_IP>:8080/v1/`
- `/v1/models` - List available models
- `/v1/chat/completions` - Chat completions
- `/health` - Health check

**Parameters:**
- `--host`: Bind address (default: 127.0.0.1, use 0.0.0.0 for network access)
- `--port`: Proxy port (default: 8080)
- `--llama-port`: Starting port for llama-server instances (default: 5500)
- `--default-context`: Default context size (default: 4096)

**Features:**
- Automatic model server management (start/stop on demand)
- Request routing to appropriate llama-server instances
- Full OpenAI SDK compatibility
- Seamless integration with OpenWebUI

**OpenWebUI Integration:**

To use GGLib with OpenWebUI:

1. Start the proxy with network access: `gglib proxy --host 0.0.0.0 --port 8080`
2. In OpenWebUI settings, configure:
   - API Base URL: `http://localhost:8080/v1`
   - API Key: (any value, not validated)
3. Select models from the dropdown - GGLib will automatically start the appropriate llama-server

For details on developing the desktop GUI, see [src-tauri/README.md](src-tauri/README.md).

The desktop build embeds the same REST API that powers `gglib gui web`. It binds to `http://localhost:8888` by default; make sure that port is free before running `gglib gui`. You can pick a different port by setting `GGLIB_GUI_API_PORT=<port>` before launching. The desktop UI now detects the port at runtime, so no frontend rebuild is required.

### Testing

```bash
export OPENAI_BASE_URL="http://localhost:8080/v1"
export OPENAI_API_KEY="dummy"
```

Then use any OpenAI-compatible client library as you normally would.

### Security Considerations

**Network Binding:**
- Web server binds to `0.0.0.0` by default (network accessible)
- Proxy binds to `127.0.0.1` by default (local only)
- Use `--host 0.0.0.0` for network access to proxy

**Authentication:**
- No authentication required by default
- Designed for trusted network environments

**Recommendations:**
- Use firewall rules to restrict access to trusted IP ranges
- Only expose on private networks (192.168.x.x, 10.x.x.x, 172.16-31.x.x)
- Use VPN for access from outside local network
- Do not port-forward to public internet without additional authentication

## 📖 Documentation

**[📚 View Full API Documentation →](https://mmogr.github.io/gglib)**

The complete API documentation is automatically generated from the source code and hosted on GitHub Pages. It includes:

- 🔍 **Detailed API reference** for all public functions and types
- 💡 **Usage examples** and code snippets
- 🏗️ **Architecture overview** of the codebase
- 🔧 **Developer guides** for contributing

The documentation is automatically updated with every release, so you'll always have access to the latest information.

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
