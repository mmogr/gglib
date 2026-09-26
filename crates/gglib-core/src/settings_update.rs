//! The two types that sit beside [`Settings`](super::Settings): a partial
//! update, and what validation refuses.
//!
//! A `#[path]` sibling of `settings.rs` rather than a section of it. The
//! struct and the enum are each a flat list that grows by one line per
//! setting, and `settings.rs` is at its size baseline with `Settings` itself
//! still to grow; splitting the lists out is the only part of that file that
//! can move without taking the `Settings` struct the surface checks parse out
//! of the file they parse.

use serde::{Deserialize, Serialize};

use crate::domain::{InferenceConfig, InferenceProfile};

use super::{Device, LoopGuardMode, RemotePairing, RemoteServe};

/// Partial settings update.
///
/// Each field is `Option<Option<T>>`:
/// - `None` = don't change this field
/// - `Some(None)` = set field to None/null
/// - `Some(Some(value))` = set field to value
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SettingsUpdate {
    pub default_download_path: Option<Option<String>>,
    pub default_context_size: Option<Option<u64>>,
    pub proxy_port: Option<Option<u16>>,
    pub llama_base_port: Option<Option<u16>>,
    pub max_download_queue_size: Option<Option<u32>>,
    pub show_memory_fit_indicators: Option<Option<bool>>,
    pub max_tool_iterations: Option<Option<u32>>,
    pub max_stagnation_steps: Option<Option<u32>>,
    pub default_model_id: Option<Option<i64>>,
    pub inference_defaults: Option<Option<InferenceConfig>>,
    pub inference_profiles: Option<Option<Vec<InferenceProfile>>>,
    pub setup_completed: Option<Option<bool>>,
    pub title_generation_prompt: Option<Option<String>>,
    pub bind_host: Option<Option<String>>,
    pub share_lan: Option<Option<bool>>,
    pub proxy_api_key: Option<Option<String>>,
    pub trust_client_sampling: Option<Option<bool>>,
    /// See [`Settings::loop_guard_mode`](super::Settings::loop_guard_mode).
    /// Writing it **to a value** clears [`Self::proxy_loop_detection`], and
    /// the other way round; `Some(None)` — the explicit clear — clears only
    /// itself, because "forget this field" is not "forget both".
    pub loop_guard_mode: Option<Option<LoopGuardMode>>,
    /// **Deprecated**; see
    /// [`Settings::proxy_loop_detection`](super::Settings::proxy_loop_detection).
    pub proxy_loop_detection: Option<Option<bool>>,
    pub tool_call_repair: Option<Option<bool>>,
    /// See [`Settings::agentic_sampling`](super::Settings::agentic_sampling).
    pub agentic_sampling: Option<Option<bool>>,
    pub proxy_autostart: Option<Option<bool>>,
    pub close_to_tray: Option<Option<bool>>,
    pub start_at_login: Option<Option<bool>>,
    /// See [`Settings::remote_pairing`](super::Settings::remote_pairing).
    /// Sets, clears or leaves the whole record: there is no field here for
    /// half of it. The pairing's own writers, `join` and a `--remote`
    /// turn, do not come through here; they change the record inside
    /// [`SettingsRepository::modify`](crate::ports::SettingsRepository::modify),
    /// as it stands when the write lands.
    pub remote_pairing: Option<Option<RemotePairing>>,
    /// See [`Settings::remote_enabled`](super::Settings::remote_enabled).
    pub remote_enabled: Option<Option<bool>>,
    /// See [`Settings::remote_serve`](super::Settings::remote_serve). Written whole.
    pub remote_serve: Option<Option<RemoteServe>>,
    /// See [`Settings::remote_devices`](super::Settings::remote_devices). Written whole.
    pub remote_devices: Option<Option<Vec<Device>>>,
}

/// Settings validation error.
#[derive(Debug, Clone, thiserror::Error)]
pub enum SettingsError {
    #[error("Context size must be between 512 and 1,000,000, got {0}")]
    InvalidContextSize(u64),

    #[error("Port should be >= 1024 (privileged ports require root), got {0}")]
    InvalidPort(u16),

    #[error("Max download queue size must be between 1 and 50, got {0}")]
    InvalidQueueSize(u32),

    #[error("Download path cannot be empty")]
    EmptyDownloadPath,

    #[error("Invalid inference parameter: {0}")]
    InvalidInferenceConfig(String),

    #[error("Invalid inference profile: {0}")]
    InvalidInferenceProfile(String),

    #[error("Bind host must be an IP address (e.g. 127.0.0.1 or 0.0.0.0), got '{0}'")]
    InvalidBindHost(String),

    #[error("Proxy API key cannot be blank — clear it instead to disable authentication")]
    BlankProxyApiKey,

    #[error("Remote API key cannot be blank — clear the pairing instead to forget it")]
    BlankRemoteApiKey,

    #[error("Remote ticket cannot be blank — clear the pairing instead to forget it")]
    BlankRemoteTicket,

    /// An id the tunnel edge would refuse to hold a token under.
    #[error("Device id {0:?} must be 1-64 of ASCII letters, digits, '.', '_' or '-'")]
    InvalidDeviceId(String),
}
