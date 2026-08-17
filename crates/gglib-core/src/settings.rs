//! Settings domain types and validation.
//!
//! This module contains the core settings types used across the application.
//! These are pure domain types with no infrastructure dependencies.

use serde::{Deserialize, Serialize};

use crate::domain::{InferenceConfig, InferenceProfile};

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

/// Application settings structure.
///
/// All fields are optional to support partial updates and graceful defaults.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Settings {
    /// Default directory for downloading models.
    pub default_download_path: Option<String>,

    /// Default context size for models (e.g., 4096, 8192).
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
    /// replayed history already repeats the same tool-call batch or the same
    /// assistant response beyond the shared agent-path thresholds is rejected
    /// with a clean HTTP 400 (`loop_detected` / `stagnation_detected`)
    /// **before** admission — no model swap, no generation, no ten minutes of
    /// scrolling garbage. `Some(false)` disables the guard entirely: the
    /// escape hatch for a client that legitimately replays identical
    /// tool-call batches or responses.
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
}

impl Settings {
    /// Create settings with sensible defaults.
    #[must_use]
    pub const fn with_defaults() -> Self {
        Self {
            default_download_path: None,
            default_context_size: Some(DEFAULT_CONTEXT_SIZE),
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
}

/// Validate settings values.
pub fn validate_settings(settings: &Settings) -> Result<(), SettingsError> {
    // Validate context size
    if let Some(ctx_size) = settings.default_context_size
        && !(512..=1_000_000).contains(&ctx_size)
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

/// Validate a set of inference profiles.
///
/// Checks each profile's name against
/// [`crate::domain::inference_profile::validate_name`], rejects
/// duplicate names (they would make `{model}:{profile}` ambiguous), and reuses
/// [`validate_inference_config`] for the numeric ranges so profile parameters
/// and global defaults can never drift apart on what counts as valid.
///
/// # Errors
///
/// Returns a human-readable description of the first problem found.
pub fn validate_inference_profiles(profiles: &[InferenceProfile]) -> Result<(), String> {
    let mut seen: Vec<&str> = Vec::with_capacity(profiles.len());

    for profile in profiles {
        profile.validate().map_err(|e| e.to_string())?;

        if seen.contains(&profile.name.as_str()) {
            return Err(format!("duplicate profile name '{}'", profile.name));
        }
        seen.push(&profile.name);

        validate_inference_config(&profile.config)
            .map_err(|e| format!("profile '{}': {e}", profile.name))?;
    }

    Ok(())
}

/// Validate inference configuration parameters.
///
/// Checks that all specified parameters are within valid ranges.
pub fn validate_inference_config(config: &InferenceConfig) -> Result<(), String> {
    // Validate temperature (0.0 - 2.0)
    if let Some(temp) = config.temperature
        && !(0.0..=2.0).contains(&temp)
    {
        return Err(format!(
            "Temperature must be between 0.0 and 2.0, got {temp}"
        ));
    }

    // Validate top_p (0.0 - 1.0)
    if let Some(top_p) = config.top_p
        && !(0.0..=1.0).contains(&top_p)
    {
        return Err(format!("Top P must be between 0.0 and 1.0, got {top_p}"));
    }

    // Validate top_k (must be positive)
    if let Some(top_k) = config.top_k
        && top_k <= 0
    {
        return Err(format!("Top K must be positive, got {top_k}"));
    }

    // Validate max_tokens (must be positive)
    if let Some(max_tokens) = config.max_tokens
        && max_tokens == 0
    {
        return Err("Max tokens must be positive".to_string());
    }

    // Validate reasoning_budget_tokens (>= -1, exactly upstream's range —
    // llama-server answers -2 with an HTTP 400 naming it, ADR 0007 finding 7c;
    // -1 defers to the launch `--reasoning-budget` and 0 stops thinking).
    //
    // This guard is the *stored* half of a boundary the request half already
    // has. `InferenceConfig::extract_client_sampling` applies the same range to
    // a value that arrives on a request, but three surfaces deserialise a whole
    // `InferenceConfig` and never pass through it: `Settings::inference_defaults`,
    // `inference_profiles[].config`, and the proxy's `inference_override`. A
    // value stored through any of them is force-inserted into every chat body,
    // so `-5000` in global defaults means an HTTP 400 on every request to every
    // model until someone finds the setting — and neither reasoning control is
    // observable in `/slots` or `/props` (ADR 0007 finding 7a), so no readback
    // can ever point at it. Rejecting at store time is the only place this is
    // catchable.
    //
    // `reasoning_effort` needs no twin guard: it is an enum, so serde refuses
    // an unknown level before this function is reached.
    if let Some(budget) = config.reasoning_budget_tokens
        && budget < -1
    {
        return Err(format!(
            "Reasoning budget tokens must be -1 or greater \
             (-1 defers to the launch default, 0 stops thinking), got {budget}"
        ));
    }

    // Validate repeat_penalty (must be positive)
    if let Some(repeat_penalty) = config.repeat_penalty
        && repeat_penalty <= 0.0
    {
        return Err(format!(
            "Repeat penalty must be positive, got {repeat_penalty}"
        ));
    }

    // Validate presence_penalty (0.0 - 2.0)
    if let Some(pp) = config.presence_penalty
        && !(0.0..=2.0).contains(&pp)
    {
        return Err(format!(
            "Presence penalty must be between 0.0 and 2.0, got {pp}"
        ));
    }

    // Validate min_p (0.0 - 1.0)
    if let Some(mp) = config.min_p
        && !(0.0..=1.0).contains(&mp)
    {
        return Err(format!("Min P must be between 0.0 and 1.0, got {mp}"));
    }

    // Validate frequency_penalty (-2.0 - 2.0, the OpenAI-spec range llama.cpp
    // honours; negative values encourage reuse and are valid upstream)
    if let Some(fp) = config.frequency_penalty
        && !(-2.0..=2.0).contains(&fp)
    {
        return Err(format!(
            "Frequency penalty must be between -2.0 and 2.0, got {fp}"
        ));
    }

    // Validate dynatemp_range (non-negative; 0.0 disables dynamic temperature)
    if let Some(dr) = config.dynatemp_range
        && dr < 0.0
    {
        return Err(format!(
            "Dynatemp range must be non-negative (0.0 disables), got {dr}"
        ));
    }

    // Validate dynatemp_exponent (must be positive; inert without a range)
    if let Some(de) = config.dynatemp_exponent
        && de <= 0.0
    {
        return Err(format!("Dynatemp exponent must be positive, got {de}"));
    }

    // Validate top_n_sigma (-1.0 disables; llama.cpp treats any value at or
    // below zero as off, and -1.0 is its own spelling of the default)
    if let Some(ts) = config.top_n_sigma
        && ts < -1.0
    {
        return Err(format!(
            "Top-n-sigma must be -1.0 (disabled) or greater, got {ts}"
        ));
    }

    validate_dry_params(config)
}

/// The four DRY parameters' ranges, split out of [`validate_inference_config`].
///
/// Not a judgement about them — they are checked exactly as before and in the
/// same order. They are simply the one cohesive group in a function that is
/// otherwise one field per check, so lifting them is what kept the parent
/// under `clippy::too_many_lines` when `reasoning_budget_tokens` joined. Every
/// caller reaches this through the parent; nothing validates DRY alone.
fn validate_dry_params(config: &InferenceConfig) -> Result<(), String> {
    // Validate dry_multiplier (0.0 - 5.0; 0.0 disables DRY)
    if let Some(dm) = config.dry_multiplier
        && !(0.0..=5.0).contains(&dm)
    {
        return Err(format!(
            "DRY multiplier must be between 0.0 and 5.0, got {dm}"
        ));
    }

    // Validate dry_base (> 1.0; the exponent base grows the penalty with
    // matched sequence length, so a base at or below 1.0 cannot penalise)
    if let Some(db) = config.dry_base
        && db <= 1.0
    {
        return Err(format!("DRY base must be greater than 1.0, got {db}"));
    }

    // Validate dry_allowed_length (non-negative token count)
    if let Some(dal) = config.dry_allowed_length
        && dal < 0
    {
        return Err(format!(
            "DRY allowed length must be non-negative, got {dal}"
        ));
    }

    // Validate dry_penalty_last_n (0 disables; negatives are resolved by
    // llama.cpp against the context size)
    if let Some(dpn) = config.dry_penalty_last_n
        && dpn < -1
    {
        return Err(format!(
            "DRY penalty last N must be -1 or greater (0 disables), got {dpn}"
        ));
    }

    Ok(())
}

#[cfg(test)]
#[path = "settings_tests.rs"]
mod settings_tests;
