#![doc = include_str!(concat!(env!("OUT_DIR"), "/README_GENERATED.md"))]

//! Desktop glue shared by the `gglib-app` binary.
//!
//! Since the daemon consolidation the desktop app is a pure dashboard: it
//! connects to the gglib daemon's HTTP API instead of building a backend of
//! its own. What survives here is the OS-integration layer that has no HTTP
//! equivalent — Tauri event emission for the menu and the tray.

pub mod events;
