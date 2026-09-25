//! Settings domain types and validation.
//!
//! This module contains the core settings types used across the application.
//! These are pure domain types with no infrastructure dependencies.

use serde::{Deserialize, Serialize};

use crate::domain::{InferenceConfig, InferenceProfile};

#[path = "settings_loop_guard.rs"]
mod settings_loop_guard;
pub use settings_loop_guard::LoopGuardMode;

#[path = "settings_update.rs"]
mod settings_update;
pub use settings_update::{SettingsError, SettingsUpdate};

#[path = "settings_validate.rs"]
mod settings_validate;
pub use settings_validate::{validate_inference_config, validate_inference_profiles};

#[path = "settings_remote.rs"]
mod settings_remote;
pub use settings_remote::{Device, RemotePairing, RemoteServe};

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

/// The loopback port `gglib remote join` tries first for the paired
/// machine.
///
/// A client configured against it once stays configured. Clear of the proxy
/// (8080), the daemon (9887) and the llama-server range (9000 upward); taken
/// by something else, the next free port is used and remembered instead.
pub const DEFAULT_REMOTE_PORT: u16 = 8180;

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
    /// What the proxy's turn-level loop/stagnation guard does on
    /// `/v1/chat/completions` when a replayed history trips it.
    ///
    /// A conversation that repeats the same tool-call batch back to back and
    /// gets the same answer back each time, or repeats the same assistant
    /// response anywhere in the session, beyond the shared agent-path
    /// thresholds, is answered per [`LoopGuardMode`]: `note` (absent, and the
    /// default) forwards it with a note saying what repeated, `refuse` rejects
    /// it with a clean HTTP 400 before admission, and `off` does not scan.
    /// Replaying identical batches across a history does not trip it — the
    /// batch count is back to back — and a repeat whose answer changed is not
    /// counted at all.
    ///
    /// Note the polarity: absent means the guard is **on**, because it is
    /// protection the endpoint should not silently lose, unlike
    /// [`Self::trust_client_sampling`], which is authority a client must be
    /// explicitly granted.
    ///
    /// The stagnation threshold itself comes from
    /// [`Self::max_stagnation_steps`], shared with the built-in agent loop so
    /// the two paths cannot drift.
    ///
    /// Read through [`Self::effective_loop_guard_mode`], never directly: the
    /// deprecated [`Self::proxy_loop_detection`] still answers for a settings
    /// file written by an older build.
    pub loop_guard_mode: Option<LoopGuardMode>,

    /// **Deprecated**, for one release: the boolean [`Self::loop_guard_mode`]
    /// replaces.
    ///
    /// `Some(false)` still means [`LoopGuardMode::Off`]. `Some(true)` means
    /// the guard is on, which is now [`LoopGuardMode::Note`] rather than a
    /// refusal — a deliberate behaviour change for anyone who asked for the
    /// guard by name, and the point of #1052.
    ///
    /// The two never disagree on disk: [`Self::merge`] clears each when the
    /// other is **written to a value** — clearing one leaves the other alone,
    /// since an explicit null means "forget this field", not "forget both" —
    /// so precedence is only ever consulted for a settings file an older build
    /// wrote. `gglib config settings set
    /// --proxy-loop-detection false` therefore keeps working for the release
    /// it is promised, for anyone who scripted it while the guard's own 400
    /// bodies still named it.
    pub proxy_loop_detection: Option<bool>,

    /// Whether a tool call that fails schema validation is re-issued, with
    /// `tool_choice: "required"` or as a second draw under gglib's grammar.
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
    /// The machine this one paired with, and the key it issued — see
    /// [`RemotePairing`] for why those are one value and not two.
    ///
    /// Received, not chosen: `gglib remote join` redeems the far
    /// machine's one-time code through the tunnel and stores what comes back
    /// here, so later sessions need only the ticket — or nothing, since the
    /// ticket is part of the record. `gglib q --remote` and
    /// `gglib chat --remote` attach the key as the bearer. Nothing writes it
    /// by hand, and `gglib config settings show` reports the key as held or
    /// not rather than printing it; `gglib remote key --show` prints it,
    /// alone and only when asked, for a client that is not gglib.
    ///
    /// A database written before the halves were bound holds
    /// `remote_api_key` and `remote_last_ticket` as separate rows, and both
    /// are ignored — no alias, deliberately. Neither is evidence about the
    /// other, and reading the one as belonging to the other is exactly the
    /// defect this field closes; such a machine loads as never paired and
    /// pairs again, which a stale ticket already required of it.
    pub remote_pairing: Option<RemotePairing>,

    /// Reachable across restarts, and how — see [`RemoteServe`].
    pub remote_enabled: Option<bool>,
    /// See [`RemoteServe`].
    pub remote_serve: Option<RemoteServe>,
    /// The roster of paired devices, keys excluded — see [`Device`].
    pub remote_devices: Option<Vec<Device>>,
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
            loop_guard_mode: None,
            proxy_loop_detection: None,
            tool_call_repair: None,
            proxy_autostart: None,
            close_to_tray: None,
            start_at_login: None,
            remote_pairing: None,
            remote_enabled: None,
            remote_serve: None,
            remote_devices: None,
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

    /// What the loop guard does, reconciling [`Self::loop_guard_mode`] with
    /// the deprecated [`Self::proxy_loop_detection`].
    ///
    /// The new setting wins outright when present. The boolean is consulted
    /// only when it is absent, which [`Self::merge`] makes true of anything
    /// this build has *written to a value* — an explicit clear of one spelling
    /// leaves the other standing, so both can be absent and the default
    /// answers: `Some(false)` is [`LoopGuardMode::Off`], and
    /// `Some(true)` or absent is the default, [`LoopGuardMode::Note`]. An
    /// explicit old "on" therefore becomes a note rather than a refusal,
    /// which is the behaviour change #1052 exists to make.
    ///
    /// The one place this precedence is decided, so the proxy, the CLI and
    /// anything that reports the setting cannot disagree about it.
    #[must_use]
    pub const fn effective_loop_guard_mode(&self) -> LoopGuardMode {
        match (self.loop_guard_mode, self.proxy_loop_detection) {
            (Some(mode), _) => mode,
            (None, Some(false)) => LoopGuardMode::Off,
            (None, _) => LoopGuardMode::Note,
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
        // The loop guard's two spellings clear each other when one is
        // *written to a value*, in this order, so they cannot disagree on
        // disk and an update carrying both has one answer: the new setting's.
        // An explicit null clears only itself — see below — so the pair can
        // also end up both absent, which the default covers. That is what
        // keeps `--proxy-loop-detection false` working for the release it is
        // promised.
        if let Some(ref v) = other.proxy_loop_detection {
            self.proxy_loop_detection = *v;
            // Only a *write* clears the other spelling. `Some(None)` is the
            // "clear this field" update every `UpdateSettingsRequest` field
            // must support, and clearing one spelling must not silently
            // discard what the other says.
            if v.is_some() {
                self.loop_guard_mode = None;
            }
        }
        if let Some(ref v) = other.loop_guard_mode {
            self.loop_guard_mode = *v;
            if v.is_some() {
                self.proxy_loop_detection = None;
            }
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
        self.merge_remote(other);
    }
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

    settings_remote::validate_remote(settings)?;

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
#[path = "settings_loop_guard_tests.rs"]
mod settings_loop_guard_tests;

#[cfg(test)]
#[path = "settings_tests.rs"]
mod settings_tests;

#[cfg(test)]
#[path = "settings_remote_tests.rs"]
mod settings_remote_tests;
