#![doc = include_str!(concat!(env!("OUT_DIR"), "/README_GENERATED.md"))]
#![deny(unsafe_code)]

pub mod cache_lifecycle;
pub mod cache_metrics;
pub mod canonicalization;
pub mod connections;
pub mod council_proxy;
pub mod dashboard;
pub mod forward;
pub mod mcp;
pub mod metrics;
pub mod models;
pub mod profiles;
pub mod server;
pub mod settings_cache;
pub mod slot_eviction;
pub mod slots;
pub mod slots_poller;
pub mod sse_stream;
pub mod token_calibration;

pub mod upstream_health;

pub use council_proxy::{CouncilDeps, CouncilRunParams, CouncilRunnerPort};
pub use server::serve;
