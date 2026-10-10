#![doc = include_str!("README.md")]
mod app_core;
#[cfg(any(test, feature = "test-utils"))]
mod app_core_fixture;
mod attachments;
mod chat_history;
mod component_choices;
mod model_components;
mod model_import;
mod model_links;
mod model_projector;
mod model_registrar;
mod model_service;
mod model_verification;
mod model_verification_remote;
mod projector_choices;
mod settings_cache;
mod settings_service;

pub use app_core::AppCore;
pub use attachments::AttachmentService;
pub use chat_history::{ChangeError, ChatHistoryService};
pub use component_choices::component_choices;
pub use model_import::{
    HfOrigin, MAX_GENERATION_CONFIG_LOOKUPS, ModelOrigin, build_new_model, fetch_published_sampling,
};
pub use model_links::{LinkError, LinkRole};
pub use model_registrar::ModelRegistrar;
pub use model_service::{ImportMode, ModelService, RetagDiff};
pub use model_verification::{
    ModelVerificationService, OverallHealth, ShardHealth, ShardHealthReport, ShardProgress,
    UpdateCheckResult, UpdateDetails, VerificationProgress, VerificationReport,
};
pub use model_verification_remote::{RepairStarted, missing_after_repair};
pub use projector_choices::projector_choices;
pub use settings_cache::{DEFAULT_TTL as SETTINGS_CACHE_TTL, SettingsCache};
pub use settings_service::{SettingsService, TemplateInstall};
