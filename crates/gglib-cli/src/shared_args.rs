//! Shared CLI argument groups.
//!
//! Reusable `#[derive(Args)]` structs that are flattened into multiple commands
//! via `#[command(flatten)]`, eliminating duplicate field definitions across
//! `Serve`, `Chat`, and `Question`.

use clap::Args;

/// Sampling-parameter overrides common to all inference commands.
///
/// Each field is optional. When `None`, the merge hierarchy — CLI → per-model
/// defaults (user-set) → global defaults → per-model defaults (auto-detected)
/// → the class floor — fills in the value. `gglib model explain <id>` shows
/// which of those rungs actually supplied each parameter.
#[derive(Args, Debug, Clone, Default)]
pub struct SamplingArgs {
    /// Temperature for sampling (0.0-2.0, overrides model/global defaults)
    #[arg(long)]
    pub temperature: Option<f32>,
    /// Top-p sampling (0.0-1.0, overrides model/global defaults)
    #[arg(long = "top-p")]
    pub top_p: Option<f32>,
    /// Top-k sampling (overrides model/global defaults)
    #[arg(long = "top-k")]
    pub top_k: Option<i32>,
    /// Maximum tokens to generate (overrides model/global defaults)
    #[arg(long = "max-tokens")]
    pub max_tokens: Option<u32>,
    /// Repeat penalty (overrides model/global defaults)
    #[arg(long = "repeat-penalty")]
    pub repeat_penalty: Option<f32>,
    /// Presence penalty — 0.0 = disabled, 1.5 = recommended for reasoning models
    /// (overrides model/global defaults)
    #[arg(long = "presence-penalty")]
    pub presence_penalty: Option<f32>,
    /// Min-P sampling threshold — 0.0 = disabled (overrides model/global defaults)
    #[arg(long = "min-p")]
    pub min_p: Option<f32>,
    /// Frequency penalty — scales with how often a token already appeared;
    /// 0.0 = disabled, negative encourages reuse (llama.cpp default 0.0)
    #[arg(long = "frequency-penalty")]
    pub frequency_penalty: Option<f32>,
    /// DRY repetition penalty strength — 0.0 = disabled, 0.8 = a common
    /// starting point (overrides model/global defaults)
    #[arg(long = "dry-multiplier")]
    pub dry_multiplier: Option<f32>,
    /// DRY penalty base; higher grows the penalty faster on longer repeats
    /// (llama.cpp default 1.75)
    #[arg(long = "dry-base")]
    pub dry_base: Option<f32>,
    /// Sequence length in tokens DRY tolerates before penalising
    /// (llama.cpp default 2)
    #[arg(long = "dry-allowed-length")]
    pub dry_allowed_length: Option<i32>,
    /// How far back DRY scans for repeats, in tokens; 0 disables
    /// (llama.cpp default 64)
    #[arg(long = "dry-penalty-last-n")]
    pub dry_penalty_last_n: Option<i32>,
    /// Dynamic-temperature half-range: entropy-scales the effective
    /// temperature within [temp-range, temp+range]; 0.0 = disabled
    /// (llama.cpp default 0.0)
    #[arg(long = "dynatemp-range")]
    pub dynatemp_range: Option<f32>,
    /// Dynamic-temperature exponent; inert unless --dynatemp-range is set
    /// (llama.cpp default 1.0)
    #[arg(long = "dynatemp-exponent")]
    pub dynatemp_exponent: Option<f32>,
    /// Top-n-sigma: keep tokens within n sigma of the max pre-softmax logit;
    /// -1.0 = disabled (llama.cpp default -1.0)
    #[arg(long = "top-n-sigma")]
    pub top_n_sigma: Option<f32>,
    /// How hard to ask the model to think: minimal, low, medium, high, xhigh, max.
    ///
    /// Applies only to models whose chat template declares that it reads the
    /// variable; on any other model the level is dropped before the request is
    /// sent and `gglib model explain` reports it as suppressed. Pair it with
    /// `--reasoning-budget-tokens`, which llama.cpp enforces regardless.
    ///
    /// There is no `none`: upstream treats it as "erase the setting", which
    /// lets the template's own default fire (medium, on gpt-oss). Pass
    /// `--reasoning-budget-tokens 0` to stop thinking.
    #[arg(long = "reasoning-effort", value_parser = crate::reasoning_args::parse_effort)]
    pub reasoning_effort: Option<gglib_core::domain::ReasoningEffort>,
    /// Ceiling on thinking tokens before the model is cut off.
    ///
    /// A budget, not a taste: enforced by llama.cpp itself on every model, so
    /// unlike `--reasoning-effort` it applies whatever the chat template does.
    /// `-1` defers to the launch-time `--reasoning-budget`; `0` stops thinking.
    // `allow_hyphen_values` because `-1` is a documented value here, and
    // without it clap reads the leading `-` as the start of another flag and
    // reports `unexpected argument '-1'`. The parser still rejects anything
    // that is not an integer >= -1, so a mistyped flag name landing here comes
    // back as a range error rather than being swallowed as a value.
    #[arg(
        long = "reasoning-budget-tokens",
        allow_hyphen_values = true,
        value_parser = crate::reasoning_args::parse_budget
    )]
    pub reasoning_budget_tokens: Option<i32>,
}

/// Context-size and memory-lock flags common to all inference commands.
#[derive(Args, Debug, Clone, Default)]
pub struct ContextArgs {
    /// Context size override (number or 'max' for model metadata).
    /// Resolved through a 4-level fallback chain:
    /// runtime flag → per-model server_defaults (from DB) → global default → hardcoded 4096.
    #[arg(short, long)]
    pub ctx_size: Option<String>,
    /// Enable memory lock
    #[arg(long)]
    pub mlock: bool,
}

/// Retry behaviour for transient upstream failures.
///
/// Retrying is on by default; this exists to turn it off. Scripted callers
/// generally want a failure reported at once rather than absorbed, and the
/// budget itself is tuned by `GGLIB_LLM_RETRY_MAX_ATTEMPTS` /
/// `GGLIB_LLM_RETRY_DEADLINE_SECS` rather than by more flags.
#[derive(Args, Debug, Clone, Default)]
pub struct RetryArgs {
    /// Fail immediately on a transient upstream error instead of retrying.
    #[arg(long)]
    pub no_retry: bool,
}

impl RetryArgs {
    /// The policy these flags resolve to.
    ///
    /// `--no-retry` yields a one-attempt policy; otherwise the defaults with
    /// any `GGLIB_LLM_RETRY_*` overrides applied.
    #[must_use]
    pub fn into_policy(self) -> gglib_core::retry::RetryPolicy {
        if self.no_retry {
            gglib_core::retry::RetryPolicy::disabled()
        } else {
            gglib_core::retry::RetryPolicy::from_env()
        }
    }
}

impl SamplingArgs {
    /// Convert into an [`InferenceConfig`](gglib_core::domain::InferenceConfig).
    pub fn into_inference_config(self) -> gglib_core::domain::InferenceConfig {
        gglib_core::domain::InferenceConfig {
            temperature: self.temperature,
            top_p: self.top_p,
            top_k: self.top_k,
            max_tokens: self.max_tokens,
            repeat_penalty: self.repeat_penalty,
            presence_penalty: self.presence_penalty,
            min_p: self.min_p,
            dry_multiplier: self.dry_multiplier,
            dry_base: self.dry_base,
            dry_allowed_length: self.dry_allowed_length,
            dry_penalty_last_n: self.dry_penalty_last_n,
            dynatemp_range: self.dynatemp_range,
            dynatemp_exponent: self.dynatemp_exponent,
            top_n_sigma: self.top_n_sigma,
            frequency_penalty: self.frequency_penalty,
            // No `--seed` flag: these args populate stored configuration and
            // long-lived operator overrides, where a pinned seed would make
            // every request return the same text. A seed is set per request by
            // the benchmark harness, which is the only caller that wants one.
            seed: None,
            reasoning_effort: self.reasoning_effort,
            reasoning_budget_tokens: self.reasoning_budget_tokens,
        }
    }

    /// The overrides as a config, or `None` when no sampling flag was passed.
    ///
    /// An all-`None` config is not the same as no override at all: it sits at
    /// the top of the merge hierarchy, so handing one down unconditionally
    /// would announce an opinion the user never expressed.
    #[must_use]
    pub fn into_override(self) -> Option<gglib_core::domain::InferenceConfig> {
        let config = self.into_inference_config();
        (config != gglib_core::domain::InferenceConfig::default()).then_some(config)
    }
}

/// MTP (Multi-Token Prediction) speculative-decoding overrides for the `serve` command.
#[derive(Args, Debug, Clone, Default)]
pub struct MtpArgs {
    /// Number of MTP speculative draft tokens (auto-enabled when model has 'mtp' tag).
    ///
    /// Set to 0 to explicitly disable MTP even when the model supports it.
    #[arg(long)]
    pub mtp_draft_n_max: Option<u32>,
    /// Minimum acceptance probability for MTP draft tokens (default: 0.75).
    ///
    /// Only used when MTP is enabled. Lower values increase speed at the
    /// cost of output quality. Recommended range: 0.5–0.95.
    #[arg(long)]
    pub mtp_draft_p_min: Option<f32>,
}

/// KV cache flags common to the proxy-backed commands.
///
/// Flattened into both `Serve` and `Proxy` so the two parse identically —
/// `gglib serve` is the pinned mode of the same proxy stack, and a cache
/// setting that only one of them could express would be a parity gap by
/// construction.
#[derive(Args, Debug, Clone, Default)]
pub struct CacheArgs {
    /// Enable KV cache session persistence, saving/restoring llama-server
    /// slot state to disk per session.
    ///
    /// Independent of the host-RAM prompt cache, which is auto-sized on
    /// every launch whether or not this flag is set (see `--cache-ram-mb`).
    #[arg(long)]
    pub cache: bool,
    /// Directory for KV cache slot files (defaults to <app-data-dir>/slots if --cache is set and this is omitted)
    #[arg(long)]
    pub slot_dir: Option<std::path::PathBuf>,
    /// Byte budget, in GiB, for the on-disk KV cache slot file eviction
    /// sweep. Only meaningful with `--cache`.
    ///
    /// Omit to auto-size from free disk space at `--slot-dir` (a quarter
    /// of free space + the cache's own current footprint, recomputed on
    /// every sweep so it tracks disk pressure from other applications).
    /// Can also be set via `GGLIB_CACHE_DISK_GB` (e.g. in a `.env` file)
    /// without editing this flag; the flag wins if both are set.
    #[arg(long)]
    pub cache_disk_gb: Option<u64>,
}

/// Who may reach the endpoint this run puts up.
///
/// Flattened into both `Serve` and `Proxy` for the same reason [`CacheArgs`]
/// is: they are one stack in two modes, and an access control only one of them
/// could express would be a hole in whichever lacked it.
///
/// Neither value is written back to settings — both are per-run overrides. The
/// stored `proxy_api_key` is the layer beneath `--api-key`, and the proxy
/// writes to it only when it mints a key itself.
#[derive(Args, Debug, Clone, Default)]
pub struct AccessArgs {
    /// Require `Authorization: Bearer <key>` on `/v1/*` and `/mcp`.
    ///
    /// `/health` stays open so a supervisor can poll it. Omit to fall back to
    /// the stored `proxy_api_key` setting; with neither, a loopback bind runs
    /// unauthenticated and a non-loopback bind generates a key and prints it.
    ///
    /// Prefer `GGLIB_API_KEY` (e.g. in a `.env` file) over the flag — a key on
    /// the command line is visible to every process on the machine via the
    /// process list, and lands in shell history.
    #[arg(long, env = "GGLIB_API_KEY")]
    pub api_key: Option<String>,
    /// Accept this value in the `Host` header, in addition to loopback and the
    /// address bound with `--host`. Repeatable.
    ///
    /// The proxy rejects requests naming any other host, which is what stops a
    /// malicious page from reaching it by DNS rebinding. A wildcard bind
    /// (`--host 0.0.0.0`) names no reachable address, so reaching it as
    /// `gglib.lan` or `192.168.1.5` needs that name given here.
    #[arg(long = "allowed-host", value_name = "HOST")]
    pub allowed_hosts: Vec<String>,
}

/// Serve-command options that don't belong to another group.
#[derive(Args, Debug, Clone)]
pub struct ServeOptions {
    /// Force-enable Jinja template parsing for chat templates
    #[arg(long)]
    pub jinja: bool,
    /// Host the OpenAI-compatible endpoint binds to.
    ///
    /// Defaults to loopback. `0.0.0.0` accepts LAN clients, and binding off
    /// loopback generates an API key if none is configured. A wildcard bind
    /// names no reachable address, so clients reaching it by hostname or LAN
    /// IP need that value passed to `--allowed-host`.
    #[arg(long, default_value = "127.0.0.1")]
    pub host: String,
    /// Port the OpenAI-compatible endpoint listens on.
    ///
    /// The proxy dashboard is served from the same port at
    /// `/v1/proxy/status` (JSON) and `/v1/proxy/status/stream` (SSE).
    #[arg(short, long, default_value = "8080")]
    pub port: u16,
}
