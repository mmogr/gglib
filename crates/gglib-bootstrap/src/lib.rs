//! Shared composition root for gglib adapters.
//!
//! This crate consolidates the common infrastructure-wiring steps that every
//! adapter (CLI, Axum, Tauri) needs:
//!
//! 1. Database pool + repository set
//! 2. Process runner (`LlamaServerRunner`)
//! 3. GGUF parser + model-files repository + model registrar
//! 4. Download manager (accepting an injected event emitter)
//! 5. `DownloadTriggerAdapter` (bridges `DownloadManagerPort` → `DownloadTriggerPort`)
//! 6. `ModelVerificationService` + fully wired `AppCore`
//!
//! Each adapter then adds its own concerns on top of the returned [`BuiltCore`]
//! (MCP service, proxy supervisor, SSE broadcaster, 7 domain `*Ops`, etc.).
//!
//! # Hexagonal boundary
//!
//! This crate depends **only** on pure infrastructure crates
//! (`gglib-core`, `gglib-db`, `gglib-download`, `gglib-gguf`, `gglib-hf`,
//! `gglib-runtime`). It does **not** depend on adapter crates (`gglib-mcp`,
//! `gglib-axum`, `gglib-tauri`, `gglib-cli`, `gglib-app-services`).
//!
//! # Example
//!
//! ```ignore
//! use std::sync::Arc;
//! use gglib_bootstrap::{BootstrapConfig, CoreBootstrap};
//! use gglib_core::ports::AppEventEmitter;
//!
//! let emitter: Arc<dyn AppEventEmitter> = Arc::new(MyEmitter::new());
//! let config = BootstrapConfig {
//!     db_path: database_path()?,
//!     llama_server_path: llama_server_path()?,
//!     max_concurrent: 4,
//!     models_dir: resolve_models_dir(None)?.path,
//!     hf_token: std::env::var("HF_TOKEN").ok(),
//! };
//! let core = CoreBootstrap::build(config, emitter).await?;
//! // core.app, core.runner, core.downloads, core.hf_client, … all ready
//! ```
//!
//! # Testing
//!
//! The test suite uses three layers:
//!
//! - **Unit** (`src/download_trigger.rs`): inline `#[cfg(test)]` block with a
//!   `MockDownloadManager` to verify quantization mapping and error propagation
//!   without touching the database.
//! - **Happy path / config** (`tests/build_happy_path.rs`): full
//!   `CoreBootstrap::build()` calls that confirm wiring succeeds and the
//!   returned [`BuiltCore`] is live.
//! - **Error cases** (`tests/build_error_cases.rs`): failure paths such as a
//!   missing database directory.
//! - **Functional round-trips** (`tests/functional.rs`): data round-trips
//!   through the wired repositories (model insert/list, settings save/reload,
//!   empty-state assertions for downloads, chat history, and MCP servers).
//!
//! Shared helpers in `tests/common/mod.rs` provide a `TempDir`-backed
//! `BootstrapConfig` and a [`gglib_core::ports::NoopEmitter`] so individual
//! test bodies stay to ≤ 5 lines.

#![deny(unsafe_code)]
#![deny(unused_crate_dependencies)]

// tokio is a required runtime dependency (async fn build uses it transitively)
use tokio as _;

// Suppress unused_crate_dependencies for dev-only crates used in tests/
#[cfg(test)]
use async_trait as _;
#[cfg(test)]
use chrono as _;
#[cfg(test)]
use tempfile as _;

mod builder;
mod built;
mod config;
mod download_trigger;

pub use builder::CoreBootstrap;
pub use built::BuiltCore;
pub use config::BootstrapConfig;
