#![doc = include_str!("../README.md")]
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
