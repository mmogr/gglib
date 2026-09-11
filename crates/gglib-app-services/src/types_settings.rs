//! The settings UI's DTOs: what it reads, and the partial update it writes.
//!
//! A `#[path]` child of `types.rs`, split out because that file sits exactly
//! on its ratchet baseline and the remote tunnel's settings have to go
//! somewhere. Everything here is re-exported from `types`, so no import
//! path changed. `scripts/check_settings_surfaces.sh` reads this file by
//! name for the `double_option` guard.

use serde::{Deserialize, Serialize};

/// Application settings for the settings UI.
///
/// `Default` is "nothing configured" — every field is optional, so it stands
/// for a fresh install with no saved values, which is what callers resolving
/// their own fallbacks need to test against.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub default_download_path: Option<String>,
    #[cfg_attr(feature = "ts-bindings", ts(type = "number | null"))]
    pub default_context_size: Option<u64>,
    pub proxy_port: Option<u16>,
    pub llama_base_port: Option<u16>,
    pub max_download_queue_size: Option<u32>,
    pub show_memory_fit_indicators: Option<bool>,
    pub max_tool_iterations: Option<u32>,
    pub max_stagnation_steps: Option<u32>,
    /// Default model ID for quick commands (e.g., `gglib question`).
    #[cfg_attr(feature = "ts-bindings", ts(type = "number | null"))]
    pub default_model_id: Option<i64>,
    pub inference_defaults: Option<gglib_core::domain::InferenceConfig>,
    /// Named sampling profiles, selectable per request as `{model}:{profile}`.
    pub inference_profiles: Option<Vec<gglib_core::domain::InferenceProfile>>,
    // Setup wizard
    pub setup_completed: Option<bool>,
    // Title generation
    pub title_generation_prompt: Option<String>,
    // Network binding (see `gglib_core::Settings`)
    pub bind_host: Option<String>,
    pub share_lan: Option<bool>,
    pub proxy_api_key: Option<String>,
    // Sampling authority (see `gglib_core::Settings`)
    pub trust_client_sampling: Option<bool>,
    // Proxy loop guard; `None` means enabled (see `gglib_core::Settings`)
    pub proxy_loop_detection: Option<bool>,
    /// Whether a tool call failing schema validation is re-issued with
    /// `tool_choice: "required"`. Absent means on.
    pub tool_call_repair: Option<bool>,
    /// Whether structured-output turns get their temperature capped when no
    /// human chose one. Absent means on (see `gglib_core::Settings`). Was
    /// write-only until the GUI grew a toggle — a toggle that saves but
    /// cannot read back silently resets on every reopen.
    pub agentic_sampling: Option<bool>,
    // Always-on proxy, desktop app only (see `gglib_core::Settings`)
    pub proxy_autostart: Option<bool>,
    pub close_to_tray: Option<bool>,
    pub start_at_login: Option<bool>,
}

impl From<gglib_core::Settings> for AppSettings {
    fn from(settings: gglib_core::Settings) -> Self {
        Self {
            default_download_path: settings.default_download_path,
            default_context_size: settings.default_context_size,
            proxy_port: settings.proxy_port,
            llama_base_port: settings.llama_base_port,
            max_download_queue_size: settings.max_download_queue_size,
            show_memory_fit_indicators: settings.show_memory_fit_indicators,
            max_tool_iterations: settings.max_tool_iterations,
            max_stagnation_steps: settings.max_stagnation_steps,
            default_model_id: settings.default_model_id,
            inference_defaults: settings.inference_defaults,
            inference_profiles: settings.inference_profiles,
            setup_completed: settings.setup_completed,
            title_generation_prompt: settings.title_generation_prompt,
            bind_host: settings.bind_host,
            share_lan: settings.share_lan,
            proxy_api_key: settings.proxy_api_key,
            trust_client_sampling: settings.trust_client_sampling,
            proxy_loop_detection: settings.proxy_loop_detection,
            tool_call_repair: settings.tool_call_repair,
            agentic_sampling: settings.agentic_sampling,
            proxy_autostart: settings.proxy_autostart,
            close_to_tray: settings.close_to_tray,
            start_at_login: settings.start_at_login,
        }
    }
}

/// Request body for updating application settings.
///
/// Every field is `Option<Option<T>>` with `serde_with::rust::double_option`
/// so an explicit JSON `null` (clear the setting) is distinguished from an
/// omitted key (leave unchanged) — the same pattern used by
/// [`UpdateModelRequest::server_defaults`](super::UpdateModelRequest::server_defaults).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct UpdateSettingsRequest {
    // Every field below is a `double_option`, which ts-rs cannot read through:
    // the nested `Option` needs an explicit type, spelled `T | null` with
    // `optional` so the emitted `field?: T | null` carries all three states —
    // absent leaves the setting alone, `null` clears it, a value sets it.
    #[cfg_attr(feature = "ts-bindings", ts(type = "string | null", optional))]
    #[serde(default, with = "serde_with::rust::double_option")]
    pub default_download_path: Option<Option<String>>,
    #[cfg_attr(feature = "ts-bindings", ts(type = "number | null", optional))]
    #[serde(default, with = "serde_with::rust::double_option")]
    pub default_context_size: Option<Option<u64>>,
    #[cfg_attr(feature = "ts-bindings", ts(type = "number | null", optional))]
    #[serde(default, with = "serde_with::rust::double_option")]
    pub proxy_port: Option<Option<u16>>,
    #[cfg_attr(feature = "ts-bindings", ts(type = "number | null", optional))]
    #[serde(default, with = "serde_with::rust::double_option")]
    pub llama_base_port: Option<Option<u16>>,
    #[cfg_attr(feature = "ts-bindings", ts(type = "number | null", optional))]
    #[serde(default, with = "serde_with::rust::double_option")]
    pub max_download_queue_size: Option<Option<u32>>,
    #[cfg_attr(feature = "ts-bindings", ts(type = "boolean | null", optional))]
    #[serde(default, with = "serde_with::rust::double_option")]
    pub show_memory_fit_indicators: Option<Option<bool>>,
    #[cfg_attr(feature = "ts-bindings", ts(type = "number | null", optional))]
    #[serde(default, with = "serde_with::rust::double_option")]
    pub max_tool_iterations: Option<Option<u32>>,
    #[cfg_attr(feature = "ts-bindings", ts(type = "number | null", optional))]
    #[serde(default, with = "serde_with::rust::double_option")]
    pub max_stagnation_steps: Option<Option<u32>>,
    /// Default model ID for quick commands (e.g., `gglib question`).
    #[cfg_attr(feature = "ts-bindings", ts(type = "number | null", optional))]
    #[serde(default, with = "serde_with::rust::double_option")]
    pub default_model_id: Option<Option<i64>>,
    // `as` rather than `type`, for the import — see `server_defaults` above.
    #[cfg_attr(
        feature = "ts-bindings",
        ts(as = "Option<gglib_core::domain::InferenceConfig>", optional = nullable)
    )]
    #[serde(default, with = "serde_with::rust::double_option")]
    pub inference_defaults: Option<Option<gglib_core::domain::InferenceConfig>>,
    /// Replaces the whole profile list. `null` clears it; an omitted key leaves
    /// it untouched, so a client updating an unrelated setting cannot drop
    /// profiles it never knew about.
    #[cfg_attr(
        feature = "ts-bindings",
        ts(as = "Option<Vec<gglib_core::domain::InferenceProfile>>", optional = nullable)
    )]
    #[serde(default, with = "serde_with::rust::double_option")]
    pub inference_profiles: Option<Option<Vec<gglib_core::domain::InferenceProfile>>>,
    // Setup wizard
    #[cfg_attr(feature = "ts-bindings", ts(type = "boolean | null", optional))]
    #[serde(default, with = "serde_with::rust::double_option")]
    pub setup_completed: Option<Option<bool>>,
    // Title generation
    #[cfg_attr(feature = "ts-bindings", ts(type = "string | null", optional))]
    #[serde(default, with = "serde_with::rust::double_option")]
    pub title_generation_prompt: Option<Option<String>>,
    // Network binding (see `gglib_core::Settings`)
    #[cfg_attr(feature = "ts-bindings", ts(type = "string | null", optional))]
    #[serde(default, with = "serde_with::rust::double_option")]
    pub bind_host: Option<Option<String>>,
    #[cfg_attr(feature = "ts-bindings", ts(type = "boolean | null", optional))]
    #[serde(default, with = "serde_with::rust::double_option")]
    pub share_lan: Option<Option<bool>>,
    #[cfg_attr(feature = "ts-bindings", ts(type = "string | null", optional))]
    #[serde(default, with = "serde_with::rust::double_option")]
    pub proxy_api_key: Option<Option<String>>,
    // Sampling authority (see `gglib_core::Settings`)
    #[cfg_attr(feature = "ts-bindings", ts(type = "boolean | null", optional))]
    #[serde(default, with = "serde_with::rust::double_option")]
    pub trust_client_sampling: Option<Option<bool>>,
    // Proxy loop guard; explicit `null` re-enables (see `gglib_core::Settings`)
    #[cfg_attr(feature = "ts-bindings", ts(type = "boolean | null", optional))]
    #[serde(default, with = "serde_with::rust::double_option")]
    pub proxy_loop_detection: Option<Option<bool>>,
    // Proxy tool-call repair; explicit `null` re-enables (see `gglib_core::Settings`)
    #[cfg_attr(feature = "ts-bindings", ts(type = "boolean | null", optional))]
    #[serde(default, with = "serde_with::rust::double_option")]
    pub tool_call_repair: Option<Option<bool>>,
    // Agentic-turn sampling; explicit `null` re-enables (see `gglib_core::Settings`)
    #[cfg_attr(feature = "ts-bindings", ts(type = "boolean | null", optional))]
    #[serde(
        default,
        alias = "toolCallFloor",
        with = "serde_with::rust::double_option"
    )]
    pub agentic_sampling: Option<Option<bool>>,
    // Always-on proxy, desktop app only (see `gglib_core::Settings`)
    #[cfg_attr(feature = "ts-bindings", ts(type = "boolean | null", optional))]
    #[serde(default, with = "serde_with::rust::double_option")]
    pub proxy_autostart: Option<Option<bool>>,
    #[cfg_attr(feature = "ts-bindings", ts(type = "boolean | null", optional))]
    #[serde(default, with = "serde_with::rust::double_option")]
    pub close_to_tray: Option<Option<bool>>,
    #[cfg_attr(feature = "ts-bindings", ts(type = "boolean | null", optional))]
    #[serde(default, with = "serde_with::rust::double_option")]
    pub start_at_login: Option<Option<bool>>,
}

impl From<UpdateSettingsRequest> for gglib_core::SettingsUpdate {
    fn from(request: UpdateSettingsRequest) -> Self {
        Self {
            default_download_path: request.default_download_path,
            default_context_size: request.default_context_size,
            proxy_port: request.proxy_port,
            llama_base_port: request.llama_base_port,
            max_download_queue_size: request.max_download_queue_size,
            show_memory_fit_indicators: request.show_memory_fit_indicators,
            max_tool_iterations: request.max_tool_iterations,
            max_stagnation_steps: request.max_stagnation_steps,
            default_model_id: request.default_model_id,
            inference_defaults: request.inference_defaults,
            inference_profiles: request.inference_profiles,
            setup_completed: request.setup_completed,
            title_generation_prompt: request.title_generation_prompt,
            bind_host: request.bind_host,
            share_lan: request.share_lan,
            proxy_api_key: request.proxy_api_key,
            trust_client_sampling: request.trust_client_sampling,
            proxy_loop_detection: request.proxy_loop_detection,
            tool_call_repair: request.tool_call_repair,
            agentic_sampling: request.agentic_sampling,
            proxy_autostart: request.proxy_autostart,
            close_to_tray: request.close_to_tray,
            start_at_login: request.start_at_login,
            // Written by `gglib remote join`, never from the settings UI.
            remote_pairing: None,
            // Written by `gglib remote enable`/`disable`. Remote access is
            // switched by the command that arms the tunnel, not by a field
            // in a settings form — a request that could set this to `true`
            // would claim a machine is reachable without anything binding.
            remote_enabled: None,
            remote_serve: None,
            // Written by inviting and forgetting devices, which mint and retire a
            // key at the tunnel edge. A settings form cannot do either.
            remote_devices: None,
        }
    }
}
