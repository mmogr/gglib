//! Core services for gglib domain logic.
//!
//! This module contains the business logic services that operate on
//! port traits rather than concrete implementations. Each service
//! encapsulates a specific domain concern.
//!
//! # Design Rules
//!
//! - Services accept trait objects (`Arc<dyn ModelRepository>`, etc.)
//! - No `sqlx`, `tokio::process`, or adapter-specific types
//! - Services are pure domain logic orchestration
//! - Error handling uses `CoreError` from ports

mod app_core;
mod model_service;
mod server_service;
mod settings_service;

pub use app_core::AppCore;
pub use model_service::ModelService;
pub use server_service::ServerService;
pub use settings_service::SettingsService;
