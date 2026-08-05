#![doc = include_str!(concat!(env!("OUT_DIR"), "/README_GENERATED.md"))]
#![deny(unsafe_code)]
#![deny(unused_crate_dependencies)]

//! Desktop glue shared by the `gglib-app` binary.
//!
//! Since the daemon consolidation the desktop app is a pure dashboard: it
//! connects to the gglib daemon's HTTP API instead of building a backend of
//! its own. What survives here is the OS-integration layer that has no HTTP
//! equivalent — Tauri event emission for menu/tray/llama-install progress.

pub mod events;
