//! Axum web server adapter for gglib.
//!
//! This crate provides an HTTP API for gglib using Axum. It is a library
//! crate only - a separate binary crate can be created later if a standalone
//! server is needed.
//!
//! # Architecture
//!
//! - `error` - HTTP error types with status code mapping
//! - `routes` - Route definitions and router construction
//! - `handlers/` - Request handlers (to be added as migrated)
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
#![deny(unused_crate_dependencies)]

// Silence unused dev-dependency warnings for planned test infrastructure
#[cfg(test)]
use tokio_test as _;

// gglib-db will be used by handlers as they are migrated
use gglib_db as _;

pub mod error;
pub mod routes;

// Re-export primary types
pub use error::HttpError;
pub use routes::create_router;
