#![doc = include_str!(concat!(env!("OUT_DIR"), "/README_GENERATED.md"))]
#![deny(unsafe_code)]

pub mod council_proxy;
pub mod forward;
pub mod mcp;
pub mod models;
pub mod server;

pub use council_proxy::{CouncilDeps, CouncilRunParams, CouncilRunnerPort};
pub use server::serve;
