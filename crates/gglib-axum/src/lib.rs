//! Axum web server adapter for gglib.
//!
//! This crate provides an HTTP API for gglib using Axum. It is a library
//! crate only - a separate binary crate can be created later if a standalone
//! server is needed.
//!
//! # Usage
//!
//! ```rust,ignore
//! use gglib_axum::create_router;
//! use gglib_db::CoreFactory;
//!
//! let core = CoreFactory::build_sqlite("~/.gglib/gglib.db").await?;
//! let router = create_router(core);
//! axum::serve(listener, router).await?;
//! ```

#![deny(unsafe_code)]

pub mod error;

use std::sync::Arc;

/// Create an Axum router with all API routes.
///
/// This is the main entry point for the web server adapter.
///
/// # Arguments
///
/// * `core` - The AppCore instance to use for handling requests
///
/// # Returns
///
/// An Axum Router ready to be served.
pub fn create_router<T>(_core: Arc<T>) -> axum::Router {
    // Placeholder - will be populated during extraction
    axum::Router::new()
}
