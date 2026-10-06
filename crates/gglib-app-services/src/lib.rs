#![doc = include_str!(concat!(env!("OUT_DIR"), "/README_GENERATED.md"))]
#![deny(unsafe_code)]
#![deny(unused_crate_dependencies)]

// Silence unused dependency warnings - these are used transitively
use async_trait as _;
use gglib_hf as _;
#[cfg(test)]
use tempfile as _;
use thiserror as _;
use tokio as _;
#[cfg(test)]
use tokio_test as _;
#[cfg(test)]
mod test_support;
#[cfg(test)]
mod test_support_hf;
#[cfg(test)]
mod test_support_remote;

mod error;
mod helpers;
mod hf_quantizations;
mod hub_chats;

pub mod benchmark;
#[allow(
    clippy::cast_possible_truncation,
    reason = "grandfathered at lint inheritance, #1157"
)]
mod downloads;
pub mod launch_options;
mod mcp;
mod models;
mod models_projector;
mod models_upgrade;
mod proxy;
mod proxy_guard;
mod proxy_port;
#[allow(
    clippy::significant_drop_in_scrutinee,
    clippy::significant_drop_tightening,
    clippy::single_match_else,
    reason = "grandfathered at lint inheritance, #1157"
)]
mod remote;
mod runs;
mod sampling_explain;
#[allow(
    clippy::cast_possible_truncation,
    clippy::needless_continue,
    clippy::significant_drop_tightening,
    reason = "grandfathered at lint inheritance, #1157"
)]
mod servers;
mod service_graph;
mod settings;
#[allow(
    clippy::struct_excessive_bools,
    reason = "grandfathered at lint inheritance, #1157"
)]
pub mod setup;
pub mod types;

// Primary exports
pub use error::GuiError;

// Domain ops + their Deps
pub use benchmark::BenchmarkOps;
pub use downloads::{DownloadDeps, DownloadOps};
pub use hub_chats::HubChats;
pub use mcp::McpOps;
pub use models::{ModelDeps, ModelOps};
pub use proxy::ProxyOps;
pub use remote::{
    EnableRequest, Enabled, FarCredentials, FarError, FarProxy, JoinRequest, Joined,
    OfferedPairing, PairedModels, RemoteConnection, RemoteDevice, RemoteEnableBody,
    RemoteEnableResponse, RemoteForgotten, RemoteGateway, RemoteJoinBody, RemoteJoinResponse,
    RemoteOps, RemotePeer, RemoteStatus, far_credentials,
};
pub use runs::{Reservation, Reserved, RunEnded, RunLog, RunRegistry, RunSpec, RunWork, Stopped};
pub use sampling_explain::{
    ParamProvenanceDto, ProvenanceKindDto, SamplingExplanationDto, SamplingLayerDto,
};
pub use servers::ServerOps;
pub use service_graph::{AppServices, ServiceGraphParams, build_service_graph};
pub use settings::SettingsOps;
pub use setup::{GpuInfoDto, SetupDeps, SetupOps, SetupStatus};

// Re-export commonly used types from gglib-core for convenience
pub use gglib_core::ModelFilterOptions;
pub use gglib_core::download::QueueSnapshot;
