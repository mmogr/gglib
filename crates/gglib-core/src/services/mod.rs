//! Core services - the application's business logic layer.
//!
//! This module contains high-level service abstractions that orchestrate
//! between ports (trait interfaces) and domain logic. Services here are
//! pure orchestrators - they don't know about concrete implementations.

mod app_core;
mod chat_history;
mod model_registrar;
mod model_service;
mod model_verification;
mod server_service;
mod settings_service;

pub use app_core::AppCore;
pub use chat_history::ChatHistoryService;
pub use model_registrar::{ModelFilesRepositoryPort, ModelRegistrar};
pub use model_service::ModelService;
pub use model_verification::{
    DownloadTriggerPort, ModelFilesReaderPort, ModelVerificationService, OverallHealth,
    ShardHealth, ShardHealthReport, ShardProgress, UpdateCheckResult, UpdateDetails,
    VerificationProgress, VerificationReport,
};
pub use server_service::ServerService;
pub use settings_service::SettingsService;
