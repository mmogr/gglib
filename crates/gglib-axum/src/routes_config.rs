//! Settings and system setup, nested under `/api/config`.

use axum::Router;
use axum::routing::{get, post};

use crate::handlers;
use crate::state::AppState;

/// Config and system routes: settings, setup wizard.
///
/// Nested under `/api/config` by the caller.
pub(crate) fn config_routes() -> Router<AppState> {
    Router::new()
        // Settings
        .route(
            "/settings",
            get(handlers::config::settings::get)
                .put(handlers::config::settings::update)
                .patch(handlers::config::settings::update),
        )
        // The starter profiles, as `gglib config profile install-templates`
        // adds them without `--force`.
        .route(
            "/profiles/install-templates",
            post(handlers::config::settings::install_profile_templates),
        )
        // System
        .route("/system/memory", get(handlers::config::settings::memory))
        .route(
            "/system/models-directory",
            get(handlers::config::settings::models_directory)
                .put(handlers::config::settings::update_models_directory),
        )
        .route("/system/setup-status", get(handlers::config::setup::status))
        .route(
            "/system/install-llama",
            post(handlers::config::setup::install_llama),
        )
        .route(
            "/system/llama-status",
            get(handlers::config::setup::llama_status_handler),
        )
        // POST, not GET: this runs `git fetch`.
        .route(
            "/system/llama-check-updates",
            post(handlers::config::setup::check_llama_updates),
        )
        .route(
            "/system/update-llama",
            post(handlers::config::setup::update_llama),
        )
        .route(
            "/system/uninstall-llama",
            post(handlers::config::setup::uninstall_llama_handler),
        )
        .route(
            "/system/setup-python",
            post(handlers::config::setup::setup_python),
        )
        .route(
            "/system/disable-fast-downloads",
            post(handlers::config::setup::disable_fast_downloads),
        )
        .route(
            "/system/diagnostics",
            get(handlers::config::setup::diagnostics),
        )
        .route(
            "/system/recommend-model",
            get(handlers::config::setup::recommend_model),
        )
}
