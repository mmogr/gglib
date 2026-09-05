//! Settings domain types and validation.
//!
//! This module contains the core settings types used across the application.
//! These are pure domain types with no infrastructure dependencies.

use serde::{Deserialize, Serialize};

use crate::domain::{InferenceConfig, InferenceProfile};

#[path = "settings_validate.rs"]
mod settings_validate;
pub use settings_validate::{validate_inference_config, validate_inference_profiles};

/// Default port for the OpenAI-compatible proxy server.
pub const DEFAULT_PROXY_PORT: u16 = 8080;

/// Fixed loopback port for the gglib daemon's management API.
///
/// Deliberately a compile-time constant rather than a setting: the daemon is
/// the one process every client (CLI, desktop app, browser dashboard) must be
/// able to find without configuration, and a configurable port would reopen
/// the "two daemons on different ports" split-brain this constant closes.
pub const DAEMON_PORT: u16 = 9887;

/// Default base port for llama-server instance allocation.
pub const DEFAULT_LLAMA_BASE_PORT: u16 = 9000;

/// Default context size for models when not specified by the user.
pub const DEFAULT_CONTEXT_SIZE: u64 = 4096;

/// The context sizes a person is allowed to configure.
///
/// One constant because more than one surface describes this range and they
/// have to agree — [`validate_settings`] rejects anything outside it, and so do
/// the flags that write this setting or default it. Spelling the numbers out
/// separately on each is how they drift.
///
/// Not every context-size flag is bounded by it: `--ctx-size` names a
/// per-launch value rather than this setting, and `CtxSizeArg::parse` accepts
/// any `u64`. That is a separate surface with a separate contract, not an
/// omission here.
pub const CONTEXT_SIZE_RANGE: std::ops::RangeInclusive<u64> = 512..=1_000_000;

/// Application settings structure.
///
/// All fields are optional to support partial updates and graceful defaults.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Settings {
    /// Default directory for downloading models.
    pub default_download_path: Option<String>,

    /// Default context size for models (e.g., 8192, 32768).
    ///
    /// `None` means the user has chosen nothing, and is the ordinary state —
    /// it is what lets the daemon size each launch rather than pinning it.
    /// A value here is read as a number the user typed and outranks that, so
    /// nothing writes one on their behalf and `settings unset` returns it. See
    /// [`crate::server_config::resolve_context_size_with_source`] for the chain
    /// and `Self::with_defaults` for why this field is the one left unset.
    pub default_context_size: Option<u64>,

    /// Port for the OpenAI-compatible proxy server.
    pub proxy_port: Option<u16>,

    /// Base port for llama-server instance allocation (first port in range).
    /// Note: The OpenAI-compatible proxy listens on `proxy_port`.
    pub llama_base_port: Option<u16>,

    /// Maximum number of downloads that can be queued (1-50).
    pub max_download_queue_size: Option<u32>,

    /// Whether to show memory fit indicators in `HuggingFace` browser.
    pub show_memory_fit_indicators: Option<bool>,

    /// Maximum iterations for tool calling agentic loop.
    pub max_tool_iterations: Option<u32>,

    /// Maximum stagnation steps before stopping agent loop.
    pub max_stagnation_steps: Option<u32>,

    /// Default model ID for commands that support a default model.
    pub default_model_id: Option<i64>,

    /// Global inference parameter defaults.
    ///
    /// Applied when neither request nor per-model defaults are specified.
    /// If not set, hardcoded defaults are used as final fallback.
    #[serde(default)]
    pub inference_defaults: Option<InferenceConfig>,

    /// Named sampling profiles, selectable per request as `{model}:{profile}`.
    ///
    /// Global rather than per-model: one `coding` profile applies to every
    /// model, and its sparse fields fall through to that model's own
    /// `inference_defaults` for anything it does not set. See
    /// [`crate::domain::inference_profile`].
    #[serde(default)]
    pub inference_profiles: Option<Vec<InferenceProfile>>,

    // ── Setup wizard ────────────────────────────────────────────────
    /// Whether the first-run setup wizard has been completed.
    pub setup_completed: Option<bool>,

    /// Custom prompt template for generating chat titles.
    pub title_generation_prompt: Option<String>,

    // ── Network binding ─────────────────────────────────────────────
    /// Override the bind host for `gglib web`.
    ///
    /// `None` → use the compiled-in default (`127.0.0.1`). The `--host` flag
    /// takes precedence for a single run without changing this value.
    pub bind_host: Option<String>,

    /// Whether `gglib web` binds all LAN interfaces and broadcasts over mDNS.
    ///
    /// `None`/`Some(false)` → localhost-only. The `--share-lan` flag can turn
    /// this on for a single run, but cannot turn it off — clear it here.
    pub share_lan: Option<bool>,

    /// Bearer token required on the proxy's `/v1/*` and `/mcp` routes.
    ///
    /// `None` leaves the endpoint unauthenticated, which is the historical
    /// behaviour and remains the default for a loopback bind. The proxy mints
    /// one here automatically the first time it binds a non-loopback host, so
    /// an endpoint that reaches a network is never left open by omission.
    ///
    /// `--api-key` and `GGLIB_API_KEY` override this for a single run without
    /// changing it. The desktop app reads it from here — that is how the GUI
    /// dashboard authenticates against the proxy it started.
    pub proxy_api_key: Option<String>,

    // ── Sampling authority ──────────────────────────────────────────
    /// Whether a client's own sampling parameters (`temperature`, `top_p`,
    /// `top_k`, `presence_penalty`, `repeat_penalty`, `min_p`) are honoured
    /// by the proxy at all.
    ///
    /// `None`/`Some(false)` → the client's sampling opinions are dropped from
    /// the resolution hierarchy entirely; the request falls straight through
    /// to the profile / per-model / global / floor layers as if the client
    /// had sent none of them.
    ///
    /// The carve-out is a *category*, not one exception: the client's own
    /// **budgets** are unaffected either way, because a budget says what the
    /// request *is* rather than how it should sample. `max_tokens` was the
    /// only member for a long time — ignoring it would silently truncate that
    /// client's own turns — and `reasoning_budget_tokens` joined it, capping
    /// what this turn may spend thinking within a range llama.cpp itself
    /// enforces. The list is
    /// [`CLIENT_AUTHORITATIVE_KEYS`](crate::request_pipeline::CLIENT_AUTHORITATIVE_KEYS),
    /// which carries the rule for what may join it; this doc names members
    /// rather than owning them.
    ///
    /// Defaults to distrust because most clients that talk to this proxy
    /// send fixed sampling values with no user-facing control behind them —
    /// boilerplate the client always sends, not a deliberate choice by
    /// whoever is using it (VS Code Copilot's LLM Gateway hardcodes
    /// `temperature: 0` on every request, for one). Letting that boilerplate
    /// silently outrank a model's own tuned defaults and this server's
    /// global settings defeats the point of configuring either. Set `true`
    /// for a client that does expose real sampling controls to its user
    /// (`OpenWebUI`'s sliders, for instance).
    pub trust_client_sampling: Option<bool>,

    // ── Proxy loop guard ────────────────────────────────────────────
    /// Whether the proxy's turn-level loop/stagnation guard runs on
    /// `/v1/chat/completions`.
    ///
    /// `None`/`Some(true)` → active (the default): a conversation whose
    /// replayed history already repeats the same tool-call batch back to back
    /// and gets the same answer back each time, or repeats the same assistant
    /// response anywhere in the session, beyond the
    /// shared agent-path thresholds is rejected
    /// with a clean HTTP 400 (`loop_detected` / `stagnation_detected`)
    /// **before** admission — no model swap, no generation, no ten minutes of
    /// scrolling garbage. `Some(false)` disables the guard entirely: the
    /// escape hatch for a client that legitimately repeats identical
    /// tool-call batches with nothing in between, or repeats a response.
    /// Replaying identical batches across a history no longer trips it — the
    /// batch count is back to back — and a repeat whose answer changed is not
    /// counted at all.
    ///
    /// Note the inverse polarity to [`Self::trust_client_sampling`]: absent
    /// means **on**, because the guard is protection the endpoint should not
    /// silently lose, while trusting client sampling is authority a client
    /// must be explicitly granted.
    ///
    /// The stagnation threshold itself comes from
    /// [`Self::max_stagnation_steps`], shared with the built-in agent loop so
    /// the two paths cannot drift.
    pub proxy_loop_detection: Option<bool>,

    /// Whether a tool call that fails schema validation is re-issued with
    /// `tool_choice: "required"`.
    ///
    /// `None` (the default) means **on**, the same inverse polarity as
    /// [`Self::proxy_loop_detection`] and for the same reason: it is
    /// protection the endpoint should not lose silently. `Some(false)`
    /// forwards every call as emitted.
    ///
    /// Worth turning off only for a client that depends on receiving the
    /// model's literal output — the repair costs one extra generation on a
    /// failed call, and nothing on a conformant one. The
    /// `GGLIB_DISABLE_TOOL_REPAIR` environment switch reaches the same gate
    /// without persisting a setting.
    ///
    /// See [Tool-call repair](https://github.com/mmogr/gglib/blob/main/docs/tool-call-repair.md).
    pub tool_call_repair: Option<bool>,

    // ── Agentic-turn sampling ───────────────────────────────────────
    /// Whether a request carrying tools gets the agentic-turn temperature
    /// ceiling — see
    /// [`InferenceConfig::agentic_temperature_ceiling`](crate::domain::InferenceConfig::agentic_temperature_ceiling).
    ///
    /// `None`/`Some(true)` → active (the default): a turn that may emit
    /// structured output has its temperature capped, but only over a value
    /// nobody deliberately chose — an auto-detected recipe or the floor —
    /// and only on a model class that still has a ceiling. Since the
    /// 2026-08-10 measurement (see `agentic_temperature_ceiling`) reasoning
    /// models have none, so on them this setting currently gates nothing.
    /// Anything set by a person stands. `Some(false)` disables the cap.
    ///
    /// Same polarity as [`Self::proxy_loop_detection`], and for the same
    /// reason: this is a correction the endpoint should not silently lose.
    ///
    /// The `tool_call_floor` alias is the name this shipped under briefly in
    /// #741, before verification showed the adjustment fires on every agentic
    /// turn rather than only on tool emission. Kept so a config written in
    /// that window still loads.
    #[serde(alias = "tool_call_floor")]
    pub agentic_sampling: Option<bool>,

    // ── Always-on proxy (desktop app) ───────────────────────────────
    /// Whether the desktop app starts the OpenAI-compatible proxy as soon as
    /// it launches, rather than waiting for the user to switch it on.
    ///
    /// This is what makes the proxy a background service rather than a
    /// feature you remember to enable: combined with [`Self::start_at_login`]
    /// and [`Self::close_to_tray`], the endpoint is simply always there for
    /// clients like VS Code Copilot, with no terminal held open.
    ///
    /// Read by the desktop app only. `gglib proxy` and `gglib serve` are
    /// explicit foreground commands — starting a second proxy underneath them
    /// would contend for the same port.
    pub proxy_autostart: Option<bool>,

    /// Whether closing the desktop app's window hides it to the system tray
    /// instead of quitting.
    ///
    /// `None`/`Some(false)` → closing the window shuts the app down, stopping
    /// the proxy and any running llama-server with it (the historical
    /// behaviour). `Some(true)` → the window hides and the app keeps serving;
    /// quitting is then an explicit action from the tray menu.
    pub close_to_tray: Option<bool>,

    /// Whether the desktop app registers itself to launch on login.
    ///
    /// Backed by the OS autostart mechanism for each platform (macOS login
    /// item, Windows `Run` key, XDG autostart entry on Linux). Toggling this
    /// registers or unregisters immediately rather than at next launch, so the
    /// stored value and the OS state cannot drift apart.
    pub start_at_login: Option<bool>,

    // ── Remote tunnel, connect side (ADR 0012) ──────────────────────
    /// The API key of the machine this one last paired with over
    /// `gglib remote connect`.
    ///
    /// Received, not chosen: `connect` redeems the one-time pairing code for
    /// it through the tunnel and stores it here so later sessions need only
    /// the ticket. It is that machine's `proxy_api_key`, and `gglib q --remote`
    /// and `gglib chat --remote` attach it as the bearer. Nothing writes it by
    /// hand and no settings surface exposes it; `gglib remote connect` with a
    /// fresh pairing replaces it.
    pub remote_api_key: Option<String>,

    /// The ticket `gglib remote connect` last dialled, in its canonical form.
    ///
    /// Recorded so `gglib remote connect` with no argument reconnects to the
    /// same machine. It is an address, not a credential — reaching the far
    /// side still takes [`Self::remote_api_key`] — and it goes stale the
    /// moment the far side runs `enable` again, because every `enable` mints
    /// a fresh identity.
    pub remote_last_ticket: Option<String>,
}

impl Settings {
    /// Create settings with sensible defaults.
    #[must_use]
    pub const fn with_defaults() -> Self {
        Self {
            default_download_path: None,
            // `None`, not the floor. This is what `gglib config settings
            // reset` writes, and a stored value is the evidence that the user
            // chose a number — the settings modal shows an empty box when
            // unset and writes back blank. Writing 4096 here fabricated that
            // evidence, and the global-default rung outranks the fitted one,
            // so a reset pinned the user above the context #925 computes for
            // their machine. The rungs below have no such problem: nothing
            // sits under `proxy_port` or `llama_base_port` to be shadowed.
            default_context_size: None,
            proxy_port: Some(DEFAULT_PROXY_PORT),
            llama_base_port: Some(DEFAULT_LLAMA_BASE_PORT),
            max_download_queue_size: Some(10),
            show_memory_fit_indicators: Some(true),
            #[allow(clippy::cast_possible_truncation)] // compile-time constants, always < u32::MAX
            max_tool_iterations: Some(crate::domain::agent::DEFAULT_MAX_ITERATIONS as u32),
            #[allow(clippy::cast_possible_truncation)]
            max_stagnation_steps: Some(crate::domain::agent::DEFAULT_MAX_STAGNATION_STEPS as u32),
            agentic_sampling: None,
            default_model_id: None,
            inference_defaults: None,
            inference_profiles: None,
            setup_completed: None,
            title_generation_prompt: None,
            bind_host: None,
            share_lan: None,
            proxy_api_key: None,
            trust_client_sampling: None,
            proxy_loop_detection: None,
            tool_call_repair: None,
            proxy_autostart: None,
            close_to_tray: None,
            start_at_login: None,
            remote_api_key: None,
            remote_last_ticket: None,
        }
    }

    /// Get the effective proxy port (with default fallback).
    #[must_use]
    pub const fn effective_proxy_port(&self) -> u16 {
        match self.proxy_port {
            Some(port) => port,
            None => DEFAULT_PROXY_PORT,
        }
    }

    /// Get the effective llama-server base port (with default fallback).
    #[must_use]
    pub const fn effective_llama_base_port(&self) -> u16 {
        match self.llama_base_port {
            Some(port) => port,
            None => DEFAULT_LLAMA_BASE_PORT,
        }
    }

    /// Merge another settings into this one, only updating fields that are Some.
    pub fn merge(&mut self, other: &SettingsUpdate) {
        if let Some(ref path) = other.default_download_path {
            self.default_download_path.clone_from(path);
        }
        if let Some(ref ctx_size) = other.default_context_size {
            self.default_context_size = *ctx_size;
        }
        if let Some(ref port) = other.proxy_port {
            self.proxy_port = *port;
        }
        if let Some(ref port) = other.llama_base_port {
            self.llama_base_port = *port;
        }
        if let Some(ref queue_size) = other.max_download_queue_size {
            self.max_download_queue_size = *queue_size;
        }
        if let Some(ref show_fit) = other.show_memory_fit_indicators {
            self.show_memory_fit_indicators = *show_fit;
        }
        if let Some(ref iters) = other.max_tool_iterations {
            self.max_tool_iterations = *iters;
        }
        if let Some(ref steps) = other.max_stagnation_steps {
            self.max_stagnation_steps = *steps;
        }
        if let Some(ref model_id) = other.default_model_id {
            self.default_model_id = *model_id;
        }
        if let Some(ref inference_defaults) = other.inference_defaults {
            self.inference_defaults.clone_from(inference_defaults);
        }
        if let Some(ref inference_profiles) = other.inference_profiles {
            self.inference_profiles.clone_from(inference_profiles);
        }
        if let Some(ref v) = other.setup_completed {
            self.setup_completed = *v;
        }
        if let Some(ref v) = other.title_generation_prompt {
            self.title_generation_prompt.clone_from(v);
        }
        if let Some(ref v) = other.bind_host {
            self.bind_host.clone_from(v);
        }
        if let Some(ref v) = other.share_lan {
            self.share_lan = *v;
        }
        if let Some(ref v) = other.proxy_api_key {
            self.proxy_api_key.clone_from(v);
        }
        if let Some(ref v) = other.trust_client_sampling {
            self.trust_client_sampling = *v;
        }
        if let Some(v) = other.tool_call_repair {
            self.tool_call_repair = v;
        }
        if let Some(ref v) = other.proxy_loop_detection {
            self.proxy_loop_detection = *v;
        }
        if let Some(ref v) = other.agentic_sampling {
            self.agentic_sampling = *v;
        }
        if let Some(ref v) = other.proxy_autostart {
            self.proxy_autostart = *v;
        }
        if let Some(ref v) = other.close_to_tray {
            self.close_to_tray = *v;
        }
        if let Some(ref v) = other.start_at_login {
            self.start_at_login = *v;
        }
        if let Some(ref v) = other.remote_api_key {
            self.remote_api_key.clone_from(v);
        }
        if let Some(ref v) = other.remote_last_ticket {
            self.remote_last_ticket.clone_from(v);
        }
    }
}

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
    pub proxy_loop_detection: Option<Option<bool>>,
    pub tool_call_repair: Option<Option<bool>>,
    /// See [`Settings::agentic_sampling`].
    pub agentic_sampling: Option<Option<bool>>,
    pub proxy_autostart: Option<Option<bool>>,
    pub close_to_tray: Option<Option<bool>>,
    pub start_at_login: Option<Option<bool>>,
    /// See [`Settings::remote_api_key`].
    pub remote_api_key: Option<Option<String>>,
    /// See [`Settings::remote_last_ticket`].
    pub remote_last_ticket: Option<Option<String>>,
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

    #[error("Remote API key cannot be blank — clear it instead to forget the pairing")]
    BlankRemoteApiKey,

    #[error("Remote ticket cannot be blank — clear it instead to forget the pairing")]
    BlankRemoteTicket,
}

/// Validate settings values.
pub fn validate_settings(settings: &Settings) -> Result<(), SettingsError> {
    // Validate context size
    if let Some(ctx_size) = settings.default_context_size
        && !CONTEXT_SIZE_RANGE.contains(&ctx_size)
    {
        return Err(SettingsError::InvalidContextSize(ctx_size));
    }

    // Validate proxy port
    if let Some(port) = settings.proxy_port
        && port < 1024
    {
        return Err(SettingsError::InvalidPort(port));
    }

    // Validate llama-server base port
    if let Some(port) = settings.llama_base_port
        && port < 1024
    {
        return Err(SettingsError::InvalidPort(port));
    }

    // Validate max download queue size
    if let Some(queue_size) = settings.max_download_queue_size
        && !(1..=50).contains(&queue_size)
    {
        return Err(SettingsError::InvalidQueueSize(queue_size));
    }

    // Validate download path if specified
    if settings
        .default_download_path
        .as_ref()
        .is_some_and(|p| p.trim().is_empty())
    {
        return Err(SettingsError::EmptyDownloadPath);
    }

    // Validate the bind host if specified. Requiring a literal IP (rather than
    // accepting a name) keeps the value unambiguous for both the TCP bind and
    // the mDNS address record.
    if let Some(ref host) = settings.bind_host
        && host.parse::<std::net::IpAddr>().is_err()
    {
        return Err(SettingsError::InvalidBindHost(host.clone()));
    }

    // A stored blank would read as "authentication is on" while accepting
    // `Bearer ` from anyone. Clearing the field is the way to turn it off.
    if settings
        .proxy_api_key
        .as_ref()
        .is_some_and(|key| key.trim().is_empty())
    {
        return Err(SettingsError::BlankProxyApiKey);
    }

    // The connect side's stored pairing, same rule: a blank is neither a key
    // nor an address, and `connect` reading one would dial nothing with
    // nothing rather than say the pairing is gone.
    if settings
        .remote_api_key
        .as_ref()
        .is_some_and(|key| key.trim().is_empty())
    {
        return Err(SettingsError::BlankRemoteApiKey);
    }
    if settings
        .remote_last_ticket
        .as_ref()
        .is_some_and(|ticket| ticket.trim().is_empty())
    {
        return Err(SettingsError::BlankRemoteTicket);
    }

    // Validate inference defaults if specified
    if let Some(ref inference_config) = settings.inference_defaults {
        validate_inference_config(inference_config)
            .map_err(SettingsError::InvalidInferenceConfig)?;
    }

    // Validate inference profiles if specified
    if let Some(ref profiles) = settings.inference_profiles {
        validate_inference_profiles(profiles).map_err(SettingsError::InvalidInferenceProfile)?;
    }

    Ok(())
}

#[cfg(test)]
#[path = "settings_tests.rs"]
mod settings_tests;

#[cfg(test)]
#[path = "settings_remote_tests.rs"]
mod settings_remote_tests;
