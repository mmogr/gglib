//! Tauri command handlers.
//!
//! After Phase 3 HTTP API consolidation, only OS-specific commands remain:
//! - llama: Binary installation and status checks
//! - util: API discovery, menu sync, OS integration
//!
//! All business logic is exposed via HTTP API (gglib-axum).
//! See scripts/check-tauri-commands.sh for enforcement.

pub mod llama;
pub mod util;
