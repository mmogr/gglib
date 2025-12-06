//! HuggingFace Hub client for gglib.
//!
//! This crate provides a client for interacting with the HuggingFace Hub API,
//! specifically optimized for discovering and fetching GGUF model files.
//!
//! # Architecture
//!
//! - **Public API**: `HfClientConfig`, `DefaultHfClient`
//! - **Internal**: All HF-specific API types (`models.rs`), HTTP handling (`http.rs`)
//! - **Port implementation**: Implements `HfClientPort` from `gglib-core`
//!
//! # Usage
//!
//! Use `DefaultHfClient` through the `HfClientPort` trait from `gglib-core`:
//!
//! ```rust,ignore
//! use gglib_hf::{DefaultHfClient, HfClientConfig};
//! use gglib_core::ports::huggingface::HfClientPort;
//!
//! async fn example() {
//!     let client = DefaultHfClient::new(HfClientConfig::default());
//!
//!     // Use trait methods
//!     let sha = client.get_commit_sha("TheBloke/Llama-2-7B-GGUF").await;
//! }
//! ```

#![deny(unsafe_code)]
#![deny(unused_crate_dependencies)]
// Allow private types in public type alias - DefaultHfClient is meant to be used
// through the HfClientPort trait, not its internal generic structure
#![allow(private_interfaces)]

mod client;
mod config;
mod error;
mod http;
mod models;
mod parsing;
mod port;
mod url;

// ============================================================================
// Public API
// ============================================================================

// Client
pub use client::DefaultHfClient;

// Configuration
pub use config::HfClientConfig;

// Silence unused dev-dependency warnings
#[cfg(test)]
use mockall as _;
#[cfg(test)]
use tokio_test as _;
