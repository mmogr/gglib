//! Each argument of a planned launch, with the decision behind it: whose
//! choice it was, and why, in the words every surface shows beside the
//! flag.
//!
//! The arguments come from the same token list the spawn runs
//! (`command::launch_tokens`, `sd::sd_server_args`), so a flag cannot be
//! explained that would not be sent, nor sent unexplained: a token this
//! table does not know fails the plan's tests. The reasons are the one
//! place these sentences are written; `gglib model explain` prints them
//! and the inspector shows them.

use std::ffi::OsString;

use gglib_core::domain::{ChoiceSource, LaunchArg};
use gglib_core::server_config::ContextSizeSource;

use super::launch::{LaunchRequest, LlamaResolutions};
use crate::llama::args::{
    CacheRamSource, JinjaResolutionSource, KvCacheTypeSource, MtpResolutionSource,
    ReasoningFormatSource,
};
use crate::sd::SdServerConfig;

/// The decision an argument belongs to, whose it was, and why.
struct Why {
    decision: &'static str,
    source: ChoiceSource,
    reason: String,
}

fn why(decision: &'static str, source: ChoiceSource, reason: impl Into<String>) -> Why {
    Why {
        decision,
        source,
        reason: reason.into(),
    }
}

fn arg(flag: &str, value: Option<&OsString>, why: Why) -> LaunchArg {
    LaunchArg {
        flag: flag.to_owned(),
        value: value.map(|v| v.to_string_lossy().into_owned()),
        decision: why.decision.to_owned(),
        source: why.source,
        reason: why.reason,
    }
}

/// llama-server's arguments for `request`, as the spawn would give them
/// with the port still to be taken, each with the decision behind it.
pub(super) fn llama_args(
    config: &gglib_core::ports::ServerConfig,
    resolved: &LlamaResolutions,
    request: &LaunchRequest,
) -> Vec<LaunchArg> {
    crate::command::launch_tokens(config, "auto")
        .iter()
        .map(|token| {
            arg(
                token.flag,
                token.value.as_ref(),
                llama_why(token.flag, resolved, request),
            )
        })
        .collect()
}

/// The decision behind one llama-server flag.
///
/// Exhaustive over `launch_tokens`: a flag added there and not here is a
/// flag sent with no reason, which `every_llama_flag_has_a_reason` catches.
#[expect(
    clippy::too_many_lines,
    reason = "one arm per flag the token list emits; the table reads whole"
)]
fn llama_why(flag: &str, resolved: &LlamaResolutions, request: &LaunchRequest) -> Why {
    use ChoiceSource::{Gglib, ModelAuthor, You};
    let ctx = request.context.1;
    match flag {
        "-m" => why("model", ModelAuthor, "the weights, as published"),
        "--host" => why(
            "host",
            Gglib,
            "loopback only; the endpoint is the address you share",
        ),
        "--port" => why(
            "port",
            Gglib,
            "the next free port above the base when it loads; the endpoint's port does not move",
        ),
        "--metrics" => why("metrics", Gglib, "the dashboard reads them"),
        "--parallel" => why(
            "parallel",
            Gglib,
            "one request at a time per model, so each gets the whole context; the queue holds the rest",
        ),
        "-c" => match ctx {
            ContextSizeSource::Explicit => why("ctx", You, "asked for this launch"),
            ContextSizeSource::ModelServerDefaults => why("ctx", You, "this model's own default"),
            ContextSizeSource::GlobalDefault => why("ctx", You, "your default context size"),
            ContextSizeSource::FittedToHardware => why(
                "ctx",
                Gglib,
                "the largest rung that fits beside the weights on this machine",
            ),
            ContextSizeSource::BuiltInDefault => {
                why("ctx", Gglib, "the floor; nothing else was set")
            }
        },
        "--jinja" | "--no-jinja" => match resolved.capabilities.jinja.source {
            JinjaResolutionSource::AgentTag => why(
                "templates",
                Gglib,
                "tagged agent: tool calls need the model's own template",
            ),
            JinjaResolutionSource::ExplicitTrue | JinjaResolutionSource::ExplicitFalse => {
                why("templates", You, "set on this model or this launch")
            }
            JinjaResolutionSource::Default => why("templates", Gglib, "llama-server's own default"),
        },
        "--embeddings" => why(
            "embeddings",
            Gglib,
            "tagged embedding: this server answers embeddings and nothing else",
        ),
        "--mmproj" => why("images", You, "the projector linked to this model"),
        "--reasoning-format" => match resolved.capabilities.reasoning.source {
            ReasoningFormatSource::ReasoningTag => why(
                "reasoning",
                Gglib,
                "tagged reasoning: thinking is split from the answer",
            ),
            ReasoningFormatSource::MetadataDetection => why(
                "reasoning",
                ModelAuthor,
                "the GGUF's template thinks this way",
            ),
            ReasoningFormatSource::Explicit => why("reasoning", You, "set on this launch"),
            ReasoningFormatSource::Default => why("reasoning", Gglib, "the default format"),
        },
        "--slot-save-path" => why("cache", You, "the prompt cache on disk is on"),
        "--cache-ram" => match resolved.cache_ram.source {
            CacheRamSource::Explicit => why("cache", You, "the RAM cache size you set"),
            CacheRamSource::Auto | CacheRamSource::AutoSuppressedByEnv => why(
                "cache",
                Gglib,
                "sized when loaded: free RAM after the weights, the KV cache and the other loaded models",
            ),
        },
        "--cache-reuse" => why(
            "cache",
            Gglib,
            "reuses a cached prompt past its first edited message",
        ),
        "--cache-type-k" | "--cache-type-v" => match resolved.kv_types.source {
            KvCacheTypeSource::Default => {
                why("kv", Gglib, "q8_0 halves the KV cache at no measured cost")
            }
            KvCacheTypeSource::Explicit => why("kv", You, "set on this launch"),
            KvCacheTypeSource::DisabledByEnv => why("kv", You, "GGLIB_DISABLE_KV_QUANT is set"),
        },
        "--spec-type" | "--spec-draft-n-max" | "--spec-draft-p-min" => {
            match resolved.capabilities.mtp.source {
                MtpResolutionSource::MtpTag => why(
                    "mtp",
                    Gglib,
                    "tagged mtp: the model drafts its own tokens ahead",
                ),
                MtpResolutionSource::Explicit => why("mtp", You, "set on this launch"),
                MtpResolutionSource::Default => why("mtp", Gglib, "the default"),
            }
        }
        "--load-mode" => why("mlock", You, "keep the weights in memory (--mlock)"),
        other => why(
            "unexplained",
            Gglib,
            format!("{other} is sent and nothing here says why"),
        ),
    }
}

/// `sd-server`'s arguments for `config`, with the port still to be taken,
/// each with the decision behind it. Every sd-server flag takes a value.
pub(super) fn sd_args(config: &SdServerConfig) -> Vec<LaunchArg> {
    let tokens = crate::sd::sd_server_args(config, "auto");
    tokens
        .chunks(2)
        .map(|pair| {
            let flag = pair[0].to_string_lossy().into_owned();
            let value = pair.get(1);
            let why = sd_why(&flag);
            arg(&flag, value, why)
        })
        .collect()
}

/// The decision behind one sd-server flag.
fn sd_why(flag: &str) -> Why {
    use ChoiceSource::{Gglib, ModelAuthor, You};
    match flag {
        "--diffusion-model" | "-m" => why("model", ModelAuthor, "the weights, as published"),
        "--vae" | "--clip_l" | "--t5xxl" | "--llm" => {
            why("parts", You, "a file linked to this model")
        }
        "--listen-ip" => why(
            "host",
            Gglib,
            "loopback only; the endpoint is the address you share",
        ),
        "--listen-port" => why(
            "port",
            Gglib,
            "the next free port above the base when it loads",
        ),
        "--steps" | "--cfg-scale" | "--sampling-method" | "--sigmas" | "--linear-scale"
        | "--flash-attn" | "--conv-direct" => why(
            "recipe",
            Gglib,
            "the family's recipe, read off the weights' name",
        ),
        other => why(
            "unexplained",
            Gglib,
            format!("{other} is sent and nothing here says why"),
        ),
    }
}

#[cfg(test)]
#[path = "explain_args_tests.rs"]
mod tests;
