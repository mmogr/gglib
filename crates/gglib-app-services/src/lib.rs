#![doc = include_str!(concat!(env!("OUT_DIR"), "/README_GENERATED.md"))]

#[cfg(test)]
mod test_support;
#[cfg(test)]
mod test_support_hf;
#[cfg(test)]
mod test_support_remote;

mod error;
mod helpers;
mod hf_count;
mod hf_image_preview;
mod hf_quantizations;
mod hub_chats;

pub mod benchmark;
mod downloads;
pub mod launch_explain;
pub mod launch_options;
mod mcp;
mod models;
mod models_components;
mod models_projector;
mod models_upgrade;
mod proxy;
pub mod proxy_clients;
mod proxy_door;
mod proxy_guard;
mod proxy_port;
pub mod proxy_prove;
pub mod proxy_settings;
mod remote;
mod residency_events;
mod runs;
mod sampling_explain;
mod servers;
mod service_graph;
mod settings;
pub mod setup;
pub mod transcript;
pub mod types;

// Primary exports
pub use error::GuiError;

// Domain ops + their Deps
pub use benchmark::BenchmarkOps;
pub use downloads::{DownloadDeps, DownloadOps, download_budget, search_hf_hits, search_hf_models};
pub use hub_chats::HubChats;
pub use launch_explain::LaunchExplanationDto;
pub use mcp::McpOps;
pub use models::{ModelDeps, ModelOps};
pub use proxy::{ProxyDeps, ProxyOps};
pub use proxy_clients::{ClientFacts, ClientRecipe, ClientRecipes, RecipeLine, recipes};
pub use proxy_door::DoorClosed;
pub use proxy_prove::{ProofOutcome, ProofRefusal};
pub use proxy_settings::proxy_config_from_settings;
pub use remote::{
    EnableRequest, Enabled, FarCredentials, FarError, FarProxy, JoinRequest, Joined,
    OfferedPairing, PairedModels, RemoteConnection, RemoteDevice, RemoteEnableBody,
    RemoteEnableResponse, RemoteForgotten, RemoteGateway, RemoteJoinBody, RemoteJoinResponse,
    RemoteOps, RemotePeer, RemoteStatus, far_credentials,
};
pub use runs::{Reservation, Reserved, RunEnded, RunLog, RunRegistry, RunSpec, RunWork, Stopped};
pub use sampling_explain::{
    ParamProvenanceDto, ProvenanceKindDto, PublishedDefaultDto, PublishedStateDto,
    SamplingExplanationDto, SamplingLayerDto,
};
pub use servers::{ServerDeps, ServerOps};
pub use service_graph::{AppServices, ServiceGraphParams, build_service_graph};
pub use settings::{SettingsOps, models_directory_info};
pub use setup::{EngineInstall, EngineStatus, GpuInfoDto, SetupDeps, SetupOps, SetupStatus};
