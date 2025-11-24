#![doc = include_str!(concat!(env!("OUT_DIR"), "/proxy_docs.md"))]

pub mod handler;
pub mod models;

pub use handler::{start_proxy, start_proxy_with_shutdown};
