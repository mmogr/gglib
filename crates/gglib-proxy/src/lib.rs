#![doc = include_str!(concat!(env!("OUT_DIR"), "/README_GENERATED.md"))]
#![deny(unsafe_code)]

pub mod forward;
pub mod models;
pub mod server;

pub use server::serve;
