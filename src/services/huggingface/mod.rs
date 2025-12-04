//! HuggingFace domain module.
//!
//! This module provides a clean API for interacting with the HuggingFace Hub,
//! including searching for models, fetching quantization info, and detecting
//! tool support.
//!
//! # Architecture
//!
//! The module is organized into focused submodules:
//!
//! - `client`: The main client interface (`HuggingfaceClient`)
//! - `models`: Domain types (DTOs)
//! - `error`: Error types and result alias
//! - `http_backend`: Trait-based HTTP abstraction for testability
//! - `url_builder`: Pure URL construction helpers
//! - `parsing`: Sync JSON parsing functions
//!
//! # Example
//!
//! ```rust,no_run
//! use gglib::services::huggingface::{DefaultHuggingfaceClient, HfConfig, HfSearchQuery, HfRepoRef};
//!
//! # async fn example() -> anyhow::Result<()> {
//! // Create a client with default configuration
//! let client = DefaultHuggingfaceClient::new(HfConfig::default());
//!
//! // Search for GGUF models
//! let query = HfSearchQuery::new().with_query("llama");
//! let response = client.search_models_page(&query).await?;
//!
//! for model in response.items {
//!     println!("{}: {} downloads, {} likes", model.id, model.downloads, model.likes);
//! }
//!
//! // Get quantizations for a specific model
//! let repo = HfRepoRef::parse("TheBloke/Llama-2-7B-GGUF").unwrap();
//! let quants = client.list_quantizations(&repo).await?;
//!
//! for q in quants {
//!     println!("  {}: {:.1} MB ({} files)", q.name, q.size_mb(), q.shard_count);
//! }
//! # Ok(())
//! # }
//! ```

mod client;
mod error;
mod http_backend;
mod models;
mod parsing;
mod url_builder;

// ============================================================================
// Public API
// ============================================================================

// Client
pub use client::{DefaultHuggingfaceClient, HuggingfaceClient};

// Error types
pub use error::{HfError, HfResult};

// Domain types
pub use models::{
    HfConfig, HfEntryType, HfFileEntry, HfModelSummary, HfQuantization, HfRepoRef, HfSearchQuery,
    HfSearchResponse, HfSortField, HfToolSupportResponse,
};

// HTTP backend (for testing)
pub use http_backend::{HttpBackend, ReqwestBackend};

// URL builders (for external use if needed)
pub use url_builder::{build_download_url, build_model_info_url, build_tree_url, build_tree_url_simple};

// Parsing utilities (for external use if needed)
pub use parsing::{aggregate_quantizations, filter_files_by_quantization, parse_model_summary};

// ============================================================================
// Compatibility Re-exports
// ============================================================================

/// Legacy type alias for backward compatibility.
///
/// Use `DefaultHuggingfaceClient` instead for new code.
#[deprecated(since = "0.2.0", note = "Use DefaultHuggingfaceClient instead")]
pub type HuggingFaceService = DefaultHuggingfaceClient;
