# gglib-bootstrap

Shared composition root for gglib adapters.

This crate consolidates the infrastructure-wiring steps that were previously duplicated
across the CLI, Axum, and Tauri bootstrap modules into a single
`CoreBootstrap::build(config, emitter) → BuiltCore` call.

## What it wires

1. SQLite database pool + repository set
2. `LlamaServerRunner` (process runner)
3. `GgufParser` + `ModelFilesRepository` + `ModelRegistrar`
4. Download manager (using the injected `AppEventEmitter`)
5. `DownloadTriggerAdapter` (bridges `DownloadManagerPort` → `DownloadTriggerPort`)
6. `ModelVerificationService` + fully configured `AppCore`

## Hexagonal boundary

Depends **only** on infrastructure crates:
`gglib-core`, `gglib-db`, `gglib-download`, `gglib-gguf`, `gglib-hf`, `gglib-runtime`.

Does **not** depend on adapter crates (`gglib-mcp`, `gglib-axum`, `gglib-tauri`, `gglib-cli`).

## Usage

```rust
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
