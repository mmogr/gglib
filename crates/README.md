# gglib Crates

This directory holds every crate of the workspace except the desktop app, which is `src-tauri`. The diagram below is the workspace's layer diagram: the root README and CONTRIBUTING link to it and draw none of their own. CONTRIBUTING's [Crate Boundaries](../CONTRIBUTING.md#crate-boundaries) holds the dependency rules and says which of them CI checks.

## Architecture Overview

gglib follows **hexagonal architecture** (ports & adapters) with clear separation of concerns. The diagram and the catalogue below are written by `make readmes` from `crates/layers.toml` and the crates' manifests, and `make enforce` fails while either is out of date.

<!-- crate-graph:start -->
Each band is a layer of `crates/layers.toml`, from the top down. Under the bands, each crate is listed with the workspace crates it names in a normal or build dependency, as its manifest lists them; dev-dependencies are left out.

```text
ADAPTER ──────────────────────────────────────────────────────────────────────
  axum               cli                ffi                src-tauri
FACADE ───────────────────────────────────────────────────────────────────────
  app-services       bootstrap
INFRASTRUCTURE ───────────────────────────────────────────────────────────────
  db                 download           engine-llama       gguf
  hf                 mcp                proxy              runtime
APPLICATION ──────────────────────────────────────────────────────────────────
  agent
CORE ─────────────────────────────────────────────────────────────────────────
  core
UTILITY ──────────────────────────────────────────────────────────────────────
  broadcast          build-info         test-http
TESTS ────────────────────────────────────────────────────────────────────────
  integration-tests

DEPENDS ON ───────────────────────────────────────────────────────────────────
  axum               → app-services, bootstrap, db, gguf, mcp, proxy, runtime,
                       core, broadcast, build-info
  cli                → axum, app-services, bootstrap, db, download, mcp, proxy,
                       runtime, core, build-info
  ffi                → bootstrap, engine-llama, mcp, proxy, core
  src-tauri          → axum, app-services, proxy, core, build-info
  app-services       → download, mcp, proxy, runtime, agent, core
  bootstrap          → db, download, gguf, hf, core
  db                 → core
  download           → hf, core
  engine-llama       → gguf, core
  gguf               → core
  hf                 → core
  mcp                → core, build-info
  proxy              → mcp, core, broadcast, build-info
  runtime            → mcp, proxy, agent, core
  agent              → core
  core               → none
  broadcast          → none
  build-info         → none
  test-http          → none
  integration-tests  → none
```
<!-- crate-graph:end -->

**Key Principle**: no crate depends on a layer above its own, and `gglib-core`
depends on no other gglib crate. Infrastructure crates such as `gglib-db`
implement its port traits, and `gglib-cli` and `gglib-axum` wire them together
through `gglib-bootstrap`. Two surfaces depend on a third: `gglib-cli` and
`src-tauri` use `gglib-axum` to host the daemon, the two exceptions
CONTRIBUTING's Crate Boundaries allows.

## Crate Catalog

Each row links the crate, whose own README says what it is responsible for and how it is laid out. The descriptions are the manifests' own.

<!-- crate-catalog:start -->
| Crate | Layer | Description | Badges |
|-------|-------|-------------|--------|
| [gglib-axum](gglib-axum/) | adapter | Axum web server adapter for gglib | ![Tests](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-tests.json) ![Coverage](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-coverage.json) ![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-loc.json) ![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-complexity.json) |
| [gglib-cli](gglib-cli/) | adapter | CLI interface for gglib | ![Tests](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-tests.json) ![Coverage](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-coverage.json) ![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-loc.json) ![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-complexity.json) |
| [gglib-ffi](gglib-ffi/) | adapter | The Swift-facing surface of gglib, shipped as a dynamic framework for iOS and macOS | ![Tests](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-ffi-tests.json) ![Coverage](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-ffi-coverage.json) ![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-ffi-loc.json) ![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-ffi-complexity.json) |
| [src-tauri](../src-tauri/) | adapter | The desktop app: a window, a tray and a native menu on the gglib daemon it launches or connects to | — |
| [gglib-app-services](gglib-app-services/) | facade | The backend facade gglib-axum, gglib-cli and src-tauri share, so every surface offers the same features | ![Tests](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-tests.json) ![Coverage](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-coverage.json) ![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-loc.json) ![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-complexity.json) |
| [gglib-bootstrap](gglib-bootstrap/) | facade | Shared composition root for gglib adapters — wires infrastructure without adapter concerns | ![Tests](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-bootstrap-tests.json) ![Coverage](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-bootstrap-coverage.json) ![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-bootstrap-loc.json) ![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-bootstrap-complexity.json) |
| [gglib-db](gglib-db/) | infrastructure | SQLite repository implementations for gglib | ![Tests](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-db-tests.json) ![Coverage](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-db-coverage.json) ![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-db-loc.json) ![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-db-complexity.json) |
| [gglib-download](gglib-download/) | infrastructure | Download manager for gglib - handles model downloads from HuggingFace | ![Tests](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-download-tests.json) ![Coverage](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-download-coverage.json) ![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-download-loc.json) ![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-download-complexity.json) |
| [gglib-engine-llama](gglib-engine-llama/) | infrastructure | llama.cpp in-process: a decoder, and a server on loopback that answers gglib-proxy as the pinned llama-server does | ![Tests](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-engine-llama-tests.json) ![Coverage](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-engine-llama-coverage.json) ![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-engine-llama-loc.json) ![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-engine-llama-complexity.json) |
| [gglib-gguf](gglib-gguf/) | infrastructure | GGUF file format parsing for gglib | ![Tests](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-gguf-tests.json) ![Coverage](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-gguf-coverage.json) ![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-gguf-loc.json) ![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-gguf-complexity.json) |
| [gglib-hf](gglib-hf/) | infrastructure | HuggingFace Hub client for gglib | ![Tests](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-hf-tests.json) ![Coverage](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-hf-coverage.json) ![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-hf-loc.json) ![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-hf-complexity.json) |
| [gglib-mcp](gglib-mcp/) | infrastructure | MCP (Model Context Protocol) server management for gglib | ![Tests](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-mcp-tests.json) ![Coverage](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-mcp-coverage.json) ![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-mcp-loc.json) ![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-mcp-complexity.json) |
| [gglib-proxy](gglib-proxy/) | infrastructure | OpenAI-compatible proxy server for gglib | ![Tests](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-tests.json) ![Coverage](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-coverage.json) ![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-loc.json) ![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-complexity.json) |
| [gglib-runtime](gglib-runtime/) | infrastructure | Process runtime and OS-level concerns for gglib | ![Tests](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-tests.json) ![Coverage](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-coverage.json) ![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-loc.json) ![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-complexity.json) |
| [gglib-agent](gglib-agent/) | application | Pure-domain agentic loop for gglib (no HTTP/infra dependencies) | ![Tests](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-agent-tests.json) ![Coverage](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-agent-coverage.json) ![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-agent-loc.json) ![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-agent-complexity.json) |
| [gglib-core](gglib-core/) | core | Core domain types and port definitions for gglib | ![Tests](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-tests.json) ![Coverage](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-coverage.json) ![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-loc.json) ![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-complexity.json) |
| [gglib-broadcast](gglib-broadcast/) | utility | Generic Server-Sent Events broadcast utility shared by gglib-axum and gglib-proxy | ![Tests](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-broadcast-tests.json) ![Coverage](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-broadcast-coverage.json) ![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-broadcast-loc.json) ![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-broadcast-complexity.json) |
| [gglib-build-info](gglib-build-info/) | utility | Shared build/version metadata for gglib | ![Tests](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-build-info-tests.json) ![Coverage](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-build-info-coverage.json) ![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-build-info-loc.json) ![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-build-info-complexity.json) |
| [gglib-test-http](gglib-test-http/) | utility | A fake HTTP server's connection for tests: the request is read whole before the test can answer it | ![Tests](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-test-http-tests.json) ![Coverage](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-test-http-coverage.json) ![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-test-http-loc.json) ![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-test-http-complexity.json) |
| [gglib-integration-tests](gglib-integration-tests/) | tests | Cross-crate integration tests for gglib, gglib-proxy in front of the phone's in-process runtime among them | — |
<!-- crate-catalog:end -->

## Development Guidelines

### Adding a New Feature

1. **Define domain types** in `gglib-core/src/domain/`
2. **Define port trait** in `gglib-core/src/ports/` if infrastructure needed
3. **Implement service** in `gglib-core/src/services/` for business logic
4. **Implement adapter** in appropriate infrastructure crate
5. **Add presentation** in CLI/Axum/GUI as needed

### Testing Strategy

- **Unit tests**: Test services with mock ports
- **Integration tests**: Test adapters against real infrastructure
- **End-to-end tests**: Test full flow through CLI/API

### Adding Dependencies

What a crate in each layer may depend on, and which of those rules `scripts/check_boundaries.sh` checks, is in CONTRIBUTING's [Crate Boundaries](../CONTRIBUTING.md#crate-boundaries).

## Further Reading

- [Main README](../README.md) - Project overview and getting started
- [CONTRIBUTING](../CONTRIBUTING.md) - Conventions, the dependency rules, and the [Badges Pipeline](../CONTRIBUTING.md#badges-pipeline) that writes the badges above
- Individual crate READMEs linked in table above
