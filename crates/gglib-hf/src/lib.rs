#![doc = include_str!(concat!(env!("OUT_DIR"), "/README_GENERATED.md"))]
#![deny(unsafe_code)]
#![deny(unused_crate_dependencies)]
// Not yet swept for `unreachable_pub` — see the dead-code arc. The workspace
// denies it; this crate opts out until its module tree is closed.
#![allow(unreachable_pub)]
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

// URL construction
pub use url::build_file_url;

// Silence unused dev-dependency warnings
#[cfg(test)]
use mockall as _;
#[cfg(test)]
use tokio_test as _;
