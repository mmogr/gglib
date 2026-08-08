//! Shared CLI argument groups.
//!
//! Reusable `#[derive(Args)]` structs that are flattened into multiple commands
//! via `#[command(flatten)]`, eliminating duplicate field definitions across
//! `Serve`, `Chat`, and `Question`.

use clap::Args;
use gglib_core::cache_config::KvCacheType;

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
    /// RAM budget in MiB for llama-server's own host-RAM prompt cache
    /// (`--cache-ram`) — what makes switching between conversations fast.
    ///
    /// Omit to auto-size it from total system RAM, the model's weights, and
    /// its KV footprint at the launch context size; the chosen budget and
    /// its arithmetic are logged at startup.
    ///
    /// Pass a value to override — `0` disables the cache. Set
    /// `GGLIB_DISABLE_CACHE_AUTOSIZE=1` to skip auto-sizing entirely and
    /// use llama-server's built-in default. Independent of
    /// `--cache`/`--slot-dir`.
    #[arg(long)]
    pub cache_ram_mb: Option<u64>,
    /// Minimum chunk size in tokens for KV-shift cache reuse past the first
    /// prefix divergence point (`--cache-reuse`). Helps a follow-up prompt
    /// whose earlier messages were edited or summarized (e.g. a Copilot
    /// history compaction), which plain prefix matching can't reuse at all.
    /// Omit to disable. Can be suppressed at runtime without editing this
    /// flag via `GGLIB_DISABLE_CACHE_REUSE=1`.
    #[arg(long)]
    pub cache_reuse: Option<u32>,
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
    /// Override the K cache element type (`--cache-type-k`).
    ///
    /// Omit to use the `q8_0` default, which roughly halves KV cache
    /// bytes-per-token versus llama-server's own `f16` default. Set
    /// `GGLIB_DISABLE_KV_QUANT=1` to fall back to `f16`/`f16` for any
    /// axis not explicitly overridden here.
    #[arg(long, value_parser = kv_cache_type_parser())]
    pub cache_type_k: Option<KvCacheType>,
    /// Override the V cache element type (`--cache-type-v`).
    ///
    /// Quantizing V additionally requires Flash Attention to be active —
    /// llama-server hard-errors at startup otherwise. gglib leaves
    /// `--flash-attn` at llama-server's own `auto`; if that resolves off
    /// for your model/backend, override this to `f16` or set
    /// `GGLIB_DISABLE_KV_QUANT=1`.
    #[arg(long, value_parser = kv_cache_type_parser())]
    pub cache_type_v: Option<KvCacheType>,
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

/// Value parser for `--cache-type-k`/`--cache-type-v`.
///
/// Built from [`KvCacheType::ALL`] so `--help` and shell completions list the
/// accepted values instead of leaving users to discover them from a parse
/// error. Wrapping the domain type's `FromStr` here rather than deriving
/// `ValueEnum` on it keeps clap out of `gglib-core`, which has no CLI
/// dependency and should not gain one to improve a help string.
fn kv_cache_type_parser() -> impl clap::builder::TypedValueParser {
    use clap::builder::TypedValueParser as _;

    clap::builder::PossibleValuesParser::new(KvCacheType::ALL.iter().map(|t| t.as_llama_arg())).map(
        |s| {
            // Unreachable: clap has already rejected anything outside `ALL`,
            // and every entry there round-trips through `as_llama_arg`.
            s.parse::<KvCacheType>()
                .expect("clap accepted a value outside KvCacheType::ALL")
        },
    )
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
    /// Starting port for the underlying llama-server instance
    ///
    /// `gglib serve` runs the model behind the proxy stack, so the upstream
    /// llama-server binds its own port separately from `--port`.
    #[arg(long, default_value = "5500")]
    pub llama_port: u16,
}

/// Builder for [`ConversationSettings`](gglib_core::domain::chat::ConversationSettings)
/// from CLI argument groups.
///
/// A single conversion point used by both `chat` and `q` handlers (DRY).
pub struct ConversationSettingsBuilder {
    settings: gglib_core::domain::chat::ConversationSettings,
}

impl ConversationSettingsBuilder {
    /// Start building settings from sampling and context args.
    pub fn new(sampling: &SamplingArgs, context: &ContextArgs) -> Self {
        Self {
            settings: gglib_core::domain::chat::ConversationSettings {
                temperature: sampling.temperature,
                top_p: sampling.top_p,
                top_k: sampling.top_k,
                max_tokens: sampling.max_tokens,
                repeat_penalty: sampling.repeat_penalty,
                ctx_size: context.ctx_size.clone(),
                mlock: if context.mlock { Some(true) } else { None },
                ..Default::default()
            },
        }
    }

    /// Set the model name used for this session.
    pub fn model_name(mut self, name: impl Into<String>) -> Self {
        self.settings.model_name = Some(name.into());
        self
    }

    /// Set tool-related configuration.
    pub fn tools(mut self, tools: Vec<String>, no_tools: bool) -> Self {
        self.settings.tools = tools;
        if no_tools {
            self.settings.no_tools = Some(true);
        }
        self
    }

    /// Set agent loop parameters.
    pub fn agent_params(
        mut self,
        max_iterations: Option<usize>,
        tool_timeout_ms: Option<u64>,
        max_parallel: Option<usize>,
    ) -> Self {
        self.settings.max_iterations = max_iterations;
        self.settings.tool_timeout_ms = tool_timeout_ms;
        self.settings.max_parallel = max_parallel;
        self
    }

    /// Consume the builder and return the finished settings.
    pub fn build(self) -> gglib_core::domain::chat::ConversationSettings {
        self.settings
    }
}
