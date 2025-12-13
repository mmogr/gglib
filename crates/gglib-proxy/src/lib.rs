//! OpenAI-compatible proxy server for gglib.
//!
//! This crate provides an HTTP server that accepts OpenAI API requests and
//! forwards them to llama-server instances managed by gglib-runtime.
//!
//! # Architecture
//!
//! This crate is in the **Infrastructure Layer** — it provides external API
//! compatibility by bridging OpenAI clients to internal llama-server instances.
//!
//! - Depends only on `gglib-core` (ports and domain types)
//! - No direct dependency on gglib-runtime or sqlx
//! - Receives pre-bound TcpListener from supervisor
//!
//! # Usage
//!
//! ```ignore
//! use gglib_proxy::serve;
//!
//! let listener = TcpListener::bind("127.0.0.1:11444").await?;
//! serve(listener, default_ctx, runtime_port, catalog_port, cancel_token).await?;
//! ```
//!
//! # Endpoints
//!
//! | Endpoint | Method | Description |
//! |----------|--------|-------------|
//! | `/health` | GET | Health check (always 200) |
//! | `/v1/models` | GET | List available models |
//! | `/v1/chat/completions` | POST | Chat completion (streaming/non-streaming) |
//!
//! # Design Principles
//!
//! 1. **Ports-only dependency** — Depends only on `gglib-core` (no sqlx, no gglib-runtime)
//! 2. **External binding** — `serve()` takes a pre-bound `TcpListener` from supervisor
//! 3. **Domain → API mapping** — OpenAI types live here, domain types in gglib-core

#![deny(unsafe_code)]

pub mod forward;
pub mod models;
pub mod server;

pub use server::serve;
