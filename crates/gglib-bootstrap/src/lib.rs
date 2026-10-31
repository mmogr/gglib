#![doc = include_str!(concat!(env!("OUT_DIR"), "/README_GENERATED.md"))]

mod builder;
mod built;
mod config;

pub use builder::CoreBootstrap;
pub use built::BuiltCore;
pub use config::BootstrapConfig;
