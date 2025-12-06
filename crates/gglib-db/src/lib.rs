//! SQLite repository implementations for gglib.
//!
//! This crate provides concrete implementations of the repository ports
//! defined in `gglib-core`, using SQLite as the storage backend.
//!
//! # Structure
//!
//! - `repositories` - SQLite implementations of core ports
//! - `factory` - Composition utilities for building AppCore with SQLite backends

#![deny(unsafe_code)]

pub mod factory;
pub mod repositories;

// Re-export factory for convenient access
pub use factory::CoreFactory;
