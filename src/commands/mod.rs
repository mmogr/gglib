#![doc = include_str!(concat!(env!("OUT_DIR"), "/commands_docs.md"))]

pub mod add;
pub mod assistant_ui;
pub mod chat;
pub mod check_deps;
pub mod config;
pub mod download;
pub mod gui_web;
pub mod list;
pub mod llama;
pub mod llama_args;
pub mod llama_invocation;
pub mod presentation;
pub mod remove;
pub mod serve;
pub mod update;

// Re-export update functions for easier access in tests
pub use update::{UpdateArgs, execute as update_execute};
