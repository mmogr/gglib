# gglib-bootstrap

![Tests](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-bootstrap-tests.json)
![Coverage](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-bootstrap-coverage.json)
![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-bootstrap-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-bootstrap-complexity.json)

Shared composition root for gglib adapters.

This crate consolidates the infrastructure-wiring steps that were previously duplicated
across the CLI, Axum, and Tauri bootstrap modules into a single
`CoreBootstrap::build(config, emitter) → BuiltCore` call.

## Architecture

This crate is the **Composition Root** — sitting between adapter crates and pure infrastructure, wiring all dependencies before handing a fully-configured `BuiltCore` to each adapter.

```text
┌─────────────────────────────────────────────────────────────────────────────────────┐
│                                 Adapter Layer                                       │
│   ┌───────────────┐   ┌───────────────┐   ┌───────────────┐   ┌───────────────┐    │
│   │  gglib-tauri  │   │  gglib-axum   │   │   gglib-cli   │   │  gglib-gui    │    │
│   │ (Desktop IPC) │   │  (HTTP API)   │   │   (CLI UX)    │   │ (App Services)│    │
│   └───────┬───────┘   └───────┬───────┘   └───────┬───────┘   └───────┬───────┘    │
│           │                   │                   │                   │            │
│           └───────────────────┴───────────────────┴───────────────────┘            │
│                                         │                                           │
└─────────────────────────────────────────┼───────────────────────────────────────────┘
                                          ▼
              ┌───────────────────────────────────────────────────────┐
              │              ►► gglib-bootstrap ◄◄                    │
              │        Single shared wiring call for all adapters      │
              └───────────────────────────┬───────────────────────────┘
                                          │
                                          ▼
              ┌───────────────────────────────────────────────────────┐
              │    gglib-core, gglib-db, gglib-download,              │
              │    gglib-gguf, gglib-hf, gglib-runtime                │
              │              (Infrastructure crates)                   │
              └───────────────────────────────────────────────────────┘
```

See the [Architecture Overview](../../README.md#architecture) for the complete diagram.

## What it wires

1. `SQLite` database pool + repository set
2. `LlamaServerRunner` (process runner)
3. `GgufParser` + `ModelFilesRepository` + `ModelRegistrar`
4. Download manager (using the injected `AppEventEmitter`)
5. `DownloadTriggerAdapter` (bridges `DownloadManagerPort` → `DownloadTriggerPort`)
6. `ModelVerificationService` + fully configured `AppCore`

## Internal Structure

```text
┌─────────────────────────────────────────────────────────────────────────────────────┐
│                                 gglib-bootstrap                                     │
├─────────────────────────────────────────────────────────────────────────────────────┤
│                                                                                     │
│   ┌─────────────┐  ┌─────────────┐  ┌─────────────┐  ┌────────────────────┐        │
│   │ config.rs   │  │ built.rs    │  │ builder.rs  │  │ download_trigger.rs│        │
│   │BootstrapCfg │  │ BuiltCore   │  │CoreBootstrap│  │  (private adapter) │        │
│   └─────────────┘  └─────────────┘  └─────────────┘  └────────────────────┘        │
│         └───────────────┴────────┬───────┴────────────────┘                         │
│                                  ▼                                                  │
│                 lib.rs (declares modules + re-exports)                              │
│                                                                                     │
└─────────────────────────────────────────────────────────────────────────────────────┘
```

**Exported API:** `BootstrapConfig` (input), `CoreBootstrap::build()` (async entry point), `BuiltCore` (output carrying all wired infrastructure).

## Hexagonal boundary

Depends **only** on infrastructure crates:
`gglib-core`, `gglib-db`, `gglib-download`, `gglib-gguf`, `gglib-hf`, `gglib-runtime`.

Does **not** depend on adapter crates (`gglib-mcp`, `gglib-axum`, `gglib-tauri`, `gglib-cli`).

## Design Principles

1. **No Adapter Dependencies** — Must not depend on tauri, axum, tower, or CLI crates
2. **Single Call** — All infrastructure wired via one `CoreBootstrap::build()` async call
3. **Emitter Injection** — Event emission strategy supplied by the caller (Tauri/Axum/CLI each provide their own)
4. **Owned Output** — `BuiltCore` owns all constructed values; adapters clone `Arc`s as needed

## Usage

```rust,ignore
use std::sync::Arc;
use gglib_bootstrap::{BootstrapConfig, CoreBootstrap};
use gglib_core::paths::{database_path, llama_server_path, resolve_models_dir};

let emitter: Arc<dyn AppEventEmitter> = Arc::new(MyAdapterEmitter::new());
let config = BootstrapConfig {
    db_path: database_path()?,
    llama_server_path: llama_server_path()?,
    max_concurrent: 4,
    models_dir: resolve_models_dir(None)?.path,
    hf_token: std::env::var("HF_TOKEN").ok(),
};
let core = CoreBootstrap::build(config, emitter).await?;
// core.app, core.runner, core.downloads, core.hf_client … all ready
```
