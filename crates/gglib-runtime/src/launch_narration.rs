//! Assembling a [`LaunchNarration`] from the resolutions a launch performed.
//!
//! Every value here arrives from a resolver that already ran — this module
//! chooses wording and ordering, and computes nothing. The one rule worth
//! stating: if a decision is not gglib's to make, it does not get a line.
//! The GPU layer split is the notable case (gglib never emits `-ngl`, so the
//! offload is llama.cpp's own choice); [`backend`] reports only which
//! acceleration the installed llama build was compiled for, which gglib does
//! determine.
//!
//! Ordering is fixed here rather than at each display surface so the CLI
//! banner, `GET /v1/proxy/status`, and the GUI dashboard cannot disagree
//! about what a launch decided.

use gglib_core::cache_config::KvCacheType;
use gglib_core::domain::{
    LaunchDecision, LaunchNarration, RuntimeFlags, estimate_kv_bytes_for_context, format_gib,
    format_mib_as_gib, kv_bytes_per_token,
};
use gglib_core::normalize::tags::FORMAT_QWEN_XML;
use gglib_core::ports::ModelLaunchSpec;
use gglib_core::server_config::ContextSizeSource;

use crate::llama::args::{
    CacheRamResolution, CacheRamSource, JinjaResolutionSource, KvCacheTypeResolution,
    KvCacheTypeSource, MtpResolutionSource, ReasoningFormatSource, SlotRestoreResolution,
};
use crate::server_config::ResolvedCapabilities;

/// Everything one launch decided, gathered at the spawn site.
///
/// A parameter object rather than a twelve-argument function: the spawn site
/// holds each of these in a local already, and a positional list of that
/// length is a standing invitation to transpose two of them.
pub(crate) struct NarrationInputs<'a> {
    /// The model being launched.
    pub spec: &'a ModelLaunchSpec,
    /// Resolved context size and the rung of the chain that supplied it.
    pub context: (u64, ContextSizeSource),
    /// Resolved K/V cache quantization.
    pub kv_types: KvCacheTypeResolution,
    /// Resolved host-RAM prompt cache budget.
    pub cache_ram: &'a CacheRamResolution,
    /// Whether the disk slot layer is enabled on this proxy at all.
    pub disk_cache_enabled: bool,
    /// Whether the disk slot layer can actually resume this model.
    pub slot_restore: SlotRestoreResolution,
    /// The capability flags `build_server_config` resolved.
    pub capabilities: &'a ResolvedCapabilities,
}

/// Build the narration for one launch.
#[must_use]
pub(crate) fn narrate(inputs: &NarrationInputs<'_>) -> LaunchNarration {
    let mut n = LaunchNarration::new(
        inputs.spec.name.clone(),
        inputs.spec.quantization.clone(),
        inputs.spec.file_size_bytes,
    );

    let (ctx, ctx_source) = inputs.context;
    n.push(LaunchDecision::new(
        "ctx",
        ctx.to_string(),
        ctx_source.label(),
    ));

    if let Some(backend) = backend() {
        n.push(LaunchDecision::new("backend", backend, "llama build"));
    }

    n.push(runtime_decision());

    n.push(kv_decision(inputs, ctx));
    n.push(cache_decision(inputs));

    if let Some(mtp) = mtp_decision(inputs) {
        n.push(mtp);
    }
    if let Some(flags) = flags_decision(inputs) {
        n.push(flags);
    }
    n.push(sampling_decision());
    n.push(dialect_decision(inputs));

    n
}

/// Which acceleration the installed llama.cpp build was compiled for.
///
/// Read from the build config written at install time. `None` when llama.cpp
/// was never installed through gglib or the file is unreadable — a missing
/// backend line is the honest outcome there, since gglib genuinely does not
/// know what the binary on `PATH` was built with.
fn backend() -> Option<String> {
    let path = gglib_core::paths::llama_config_path().ok()?;
    let config = crate::llama::BuildConfig::load(&path).ok()?;
    Some(config.acceleration)
}

/// Which llama.cpp build is being spawned, and what it does natively.
///
/// Read through the same probe the compensation decisions will consult, so
/// the banner cannot claim one runtime while the pipeline assumes another.
/// Unlike [`backend`] this never returns `None`: an unidentified runtime is
/// precisely the case worth printing, because it is the case where gglib
/// keeps every compensation on and the user has no other way to find out.
fn runtime_decision() -> LaunchDecision {
    let Ok(path) = gglib_core::paths::llama_server_path() else {
        return LaunchDecision::new("runtime", "unidentified", "no llama-server path");
    };

    let caps = crate::llama::probe_runtime_capabilities(&path);

    let Some(build) = caps.build else {
        return LaunchDecision::new("runtime", "unidentified", caps.version_line);
    };

    LaunchDecision::new("runtime", format!("b{build}"), native_summary(caps.flags))
}

/// How a runtime's native capabilities read on the banner.
///
/// "none" is a real answer rather than a gap: an older build genuinely offers
/// nothing to defer to, and saying so is what distinguishes it from a build
/// whose capabilities were never checked.
fn native_summary(flags: RuntimeFlags) -> String {
    let mut parts = Vec::new();

    if flags.contains(RuntimeFlags::PEG_NATIVE_TOOL_CALLS) {
        parts.push("peg-native tool calls");
    }

    if parts.is_empty() {
        "native: none".to_owned()
    } else {
        format!("native: {}", parts.join(", "))
    }
}

/// The KV line, including what quantization actually bought.
///
/// The `f16 would be N` comparison is the point: `q8_0` alone means nothing
/// to a user, while "2.1 GB, and f16 would be 4.2" is the reason the default
/// exists. Omitted when the model's metadata didn't yield a per-token
/// estimate, rather than shown against a fabricated one.
fn kv_decision(inputs: &NarrationInputs<'_>, ctx: u64) -> LaunchDecision {
    let kv = inputs.kv_types;
    let types = if kv.k == kv.v {
        kv.k.as_llama_arg().to_string()
    } else {
        format!("k={} v={}", kv.k.as_llama_arg(), kv.v.as_llama_arg())
    };

    let source = match kv.source {
        KvCacheTypeSource::Default => "default",
        KvCacheTypeSource::Explicit => "explicit override",
        KvCacheTypeSource::DisabledByEnv => "GGLIB_DISABLE_KV_QUANT",
    };

    let Some(elems) = inputs.spec.kv_elems_per_token else {
        return LaunchDecision::new("kv", types, source);
    };

    let actual = estimate_kv_bytes_for_context(kv_bytes_per_token(elems, kv.k, kv.v), ctx);
    let value = if kv.k == KvCacheType::F16 && kv.v == KvCacheType::F16 {
        format!("{types} -> {}", format_gib(actual))
    } else {
        let f16 = estimate_kv_bytes_for_context(
            kv_bytes_per_token(elems, KvCacheType::F16, KvCacheType::F16),
            ctx,
        );
        // Comma rather than parentheses: the renderer appends the provenance
        // in parens, and nesting a second pair inside it reads as a typo.
        format!(
            "{types} -> {}, f16 would be {}",
            format_gib(actual),
            format_gib(f16)
        )
    };
    LaunchDecision::new("kv", value, source)
}

/// The prompt-cache line: RAM budget plus whether the disk slot layer is live.
///
/// Both tiers on one line because they are one user-facing question — "will
/// switching conversations be fast" — answered by two independent mechanisms.
fn cache_decision(inputs: &NarrationInputs<'_>) -> LaunchDecision {
    let res = inputs.cache_ram;
    let (ram, source) = match (res.cache_ram_mb, res.source) {
        (Some(0), CacheRamSource::Explicit) => ("RAM cache off".to_string(), "explicit"),
        (Some(0), _) => (
            "RAM cache off".to_string(),
            "auto: no room after weights + KV",
        ),
        (Some(mb), CacheRamSource::Explicit) => {
            (format!("{} RAM", format_mib_as_gib(mb)), "explicit")
        }
        (Some(mb), _) => (
            format!(
                "{} RAM of {} free",
                format_mib_as_gib(mb),
                format_gib(res.total_ram_bytes)
            ),
            "auto-sized",
        ),
        (None, _) => (
            "llama.cpp default".to_string(),
            "GGLIB_DISABLE_CACHE_AUTOSIZE",
        ),
    };

    let disk = match (inputs.disk_cache_enabled, inputs.slot_restore.enabled) {
        (false, _) => "disk slots off",
        (true, true) => "disk slots on",
        // Enabled but useless for this model — stated rather than silently
        // dropped, since the user turned it on and would otherwise assume it
        // was working.
        (true, false) => "disk slots n/a for this model",
    };

    LaunchDecision::new("cache", format!("{ram} \u{b7} {disk}"), source)
}

/// The speculative-decoding line, present only when MTP is actually on.
fn mtp_decision(inputs: &NarrationInputs<'_>) -> Option<LaunchDecision> {
    let mtp = &inputs.capabilities.mtp;
    if !mtp.enabled {
        return None;
    }
    let source = match mtp.source {
        MtpResolutionSource::Explicit => "explicit",
        MtpResolutionSource::MtpTag => "mtp tag",
        MtpResolutionSource::Default => "default",
    };
    Some(LaunchDecision::new(
        "mtp",
        format!("on, {} draft tokens", mtp.draft_n_max),
        source,
    ))
}

/// The llama-server flags gglib chose on the user's behalf.
///
/// Each flag carries its own provenance inline, so the line reads
/// `--jinja (agent tag) · --reasoning-format deepseek (reasoning tag)`.
/// `None` when gglib added no flags of its own — an empty `flags` line would
/// imply a decision that was never taken.
fn flags_decision(inputs: &NarrationInputs<'_>) -> Option<LaunchDecision> {
    let caps = inputs.capabilities;
    let mut parts: Vec<String> = Vec::new();

    if caps.jinja.enabled {
        let why = match caps.jinja.source {
            JinjaResolutionSource::AgentTag => "agent tag",
            JinjaResolutionSource::ExplicitTrue => "explicit",
            // Not reachable while `enabled` is true, but matched exhaustively
            // so a new source cannot silently render as the wrong reason.
            JinjaResolutionSource::ExplicitFalse | JinjaResolutionSource::Default => "default",
        };
        parts.push(format!("--jinja ({why})"));
    }

    if let Some(format) = &caps.reasoning.format {
        let why = match caps.reasoning.source {
            ReasoningFormatSource::ReasoningTag => "reasoning tag",
            ReasoningFormatSource::MetadataDetection => "gguf metadata",
            ReasoningFormatSource::Explicit => "explicit",
            ReasoningFormatSource::Default => "default",
        };
        parts.push(format!("--reasoning-format {format} ({why})"));
    }

    if caps.embeddings {
        // Worth naming even though it is the only flag with one possible
        // reason: it is also the only flag that takes chat completions away,
        // so a user staring at a 501 needs to see it in the banner.
        parts.push("--embeddings (embedding tag)".to_string());
    }

    (!parts.is_empty()).then(|| LaunchDecision::bare("flags", parts.join(" \u{b7} ")))
}

/// Which tool-call dialect the proxy will normalize on the way out.
///
/// Always present, including the pass-through case: "this model's tool calls
/// arrive as OpenAI JSON already" is exactly as informative as naming a
/// parser, and its absence would read as a missing feature.
fn dialect_decision(inputs: &NarrationInputs<'_>) -> LaunchDecision {
    if inputs
        .spec
        .tags
        .iter()
        .any(|t| t.as_str() == FORMAT_QWEN_XML)
    {
        LaunchDecision::new(
            "dialect",
            "qwen-xml -> OpenAI tool_calls",
            format!("{FORMAT_QWEN_XML} tag"),
        )
    } else {
        LaunchDecision::new("dialect", "OpenAI tool_calls", "pass-through")
    }
}

/// Sampling is decided per request, not at launch.
///
/// Unconditional, and stated rather than omitted for the same reason
/// `dialect_decision` is: the launch used to carry seven sampler flags, and a
/// banner that simply stopped mentioning them would read as a regression or an
/// oversight. This says the absence is the decision, and where the real one is
/// made. See `llama::args::sampling`.
fn sampling_decision() -> LaunchDecision {
    LaunchDecision::new(
        "sampling",
        crate::llama::args::SAMPLING_VALUE,
        crate::llama::args::SAMPLING_SOURCE,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use gglib_core::domain::KvElemsPerToken;
    use std::path::PathBuf;

    use crate::llama::args::{
        JinjaResolution, MtpResolution, ReasoningFormatResolution, SlotRestoreSource,
    };

    fn spec(tags: &[&str]) -> ModelLaunchSpec {
        ModelLaunchSpec {
            model_sampling: gglib_core::domain::ModelSamplingDefaults::default(),
            id: 7,
            name: "qwen3-30b-a3b".to_string(),
            file_path: PathBuf::from("/models/q.gguf"),
            tags: tags.iter().map(|t| (*t).to_string()).collect(),
            architecture: None,
            quantization: Some("Q4_K_M".to_string()),
            context_length: None,
            server_defaults: None,
            file_size_bytes: 18_476_297_420,
            kv_elems_per_token: Some(KvElemsPerToken { k: 8192, v: 8192 }),
            kv_memory_is_partial: false,
        }
    }

    fn caps() -> ResolvedCapabilities {
        ResolvedCapabilities {
            jinja: JinjaResolution {
                enabled: false,
                source: JinjaResolutionSource::Default,
            },
            reasoning: ReasoningFormatResolution {
                format: None,
                source: ReasoningFormatSource::Default,
            },
            mtp: MtpResolution {
                enabled: false,
                draft_n_max: 0,
                draft_p_min: 0.0,
                source: MtpResolutionSource::Default,
            },
            embeddings: false,
        }
    }

    fn cache_ram(mb: Option<u64>, source: CacheRamSource) -> CacheRamResolution {
        CacheRamResolution {
            cache_ram_mb: mb,
            source,
            total_ram_bytes: 33_285_996_544,
            model_bytes: 18_476_297_420,
            kv_bytes: 0,
            kv_estimated: true,
            context_size: 32_768,
        }
    }

    fn inputs<'a>(
        spec: &'a ModelLaunchSpec,
        caps: &'a ResolvedCapabilities,
        ram: &'a CacheRamResolution,
    ) -> NarrationInputs<'a> {
        NarrationInputs {
            spec,
            context: (32_768, ContextSizeSource::ModelServerDefaults),
            kv_types: KvCacheTypeResolution {
                k: KvCacheType::Q8_0,
                v: KvCacheType::Q8_0,
                source: KvCacheTypeSource::Default,
            },
            cache_ram: ram,
            disk_cache_enabled: true,
            slot_restore: SlotRestoreResolution {
                enabled: true,
                source: SlotRestoreSource::Supported,
            },
            capabilities: caps,
        }
    }

    #[test]
    fn context_line_names_the_rung_that_won() {
        let (s, c, r) = (
            spec(&[]),
            caps(),
            cache_ram(Some(6144), CacheRamSource::Auto),
        );
        let n = narrate(&inputs(&s, &c, &r));
        let ctx = n.decision("ctx").unwrap();
        assert_eq!(ctx.value, "32768");
        assert_eq!(ctx.source.as_deref(), Some("model server_defaults"));
    }

    /// The f16 comparison is what makes the KV default legible — without it
    /// the line is a magic string.
    #[test]
    fn kv_line_shows_what_quantization_saved() {
        let (s, c, r) = (
            spec(&[]),
            caps(),
            cache_ram(Some(6144), CacheRamSource::Auto),
        );
        let n = narrate(&inputs(&s, &c, &r));
        let kv = n.decision("kv").unwrap();
        assert!(kv.value.starts_with("q8_0 -> "), "got {}", kv.value);
        assert!(kv.value.contains("f16 would be"), "got {}", kv.value);
        assert_eq!(kv.source.as_deref(), Some("default"));
    }

    /// An f16 launch has nothing to compare against — it *is* the baseline.
    #[test]
    fn kv_line_omits_the_comparison_when_already_f16() {
        let (s, c, r) = (
            spec(&[]),
            caps(),
            cache_ram(Some(6144), CacheRamSource::Auto),
        );
        let mut i = inputs(&s, &c, &r);
        i.kv_types = KvCacheTypeResolution {
            k: KvCacheType::F16,
            v: KvCacheType::F16,
            source: KvCacheTypeSource::DisabledByEnv,
        };
        let n = narrate(&i);
        let kv = n.decision("kv").unwrap();
        assert!(!kv.value.contains("f16 would be"), "got {}", kv.value);
        assert_eq!(kv.source.as_deref(), Some("GGLIB_DISABLE_KV_QUANT"));
    }

    /// Without a per-token estimate there is no honest byte figure to show.
    #[test]
    fn kv_line_degrades_to_the_bare_types_without_an_estimate() {
        let mut s = spec(&[]);
        s.kv_elems_per_token = None;
        let (c, r) = (caps(), cache_ram(Some(6144), CacheRamSource::Auto));
        let n = narrate(&inputs(&s, &c, &r));
        assert_eq!(n.decision("kv").unwrap().value, "q8_0");
    }

    #[test]
    fn cache_line_reports_the_auto_budget_against_free_ram() {
        let (s, c, r) = (
            spec(&[]),
            caps(),
            cache_ram(Some(6144), CacheRamSource::Auto),
        );
        let n = narrate(&inputs(&s, &c, &r));
        let cache = n.decision("cache").unwrap();
        assert_eq!(
            cache.value,
            "6.0 GB RAM of 31.0 GB free \u{b7} disk slots on"
        );
        assert_eq!(cache.source.as_deref(), Some("auto-sized"));
    }

    /// A zero budget the machine forced reads differently from one the user
    /// chose — the whole reason `CacheRamSource` is carried this far.
    #[test]
    fn cache_line_distinguishes_a_forced_zero_from_a_chosen_one() {
        let s = spec(&[]);
        let c = caps();
        let forced = cache_ram(Some(0), CacheRamSource::Auto);
        let chosen = cache_ram(Some(0), CacheRamSource::Explicit);
        assert_eq!(
            narrate(&inputs(&s, &c, &forced))
                .decision("cache")
                .unwrap()
                .source
                .as_deref(),
            Some("auto: no room after weights + KV")
        );
        assert_eq!(
            narrate(&inputs(&s, &c, &chosen))
                .decision("cache")
                .unwrap()
                .source
                .as_deref(),
            Some("explicit")
        );
    }

    /// Enabling the disk layer for a model that cannot use it is precisely
    /// the case a user would otherwise misread as working.
    #[test]
    fn cache_line_flags_a_disk_layer_the_model_cannot_use() {
        let (s, c, r) = (
            spec(&[]),
            caps(),
            cache_ram(Some(6144), CacheRamSource::Auto),
        );
        let mut i = inputs(&s, &c, &r);
        i.slot_restore = SlotRestoreResolution {
            enabled: false,
            source: SlotRestoreSource::UnsupportedPartialKv,
        };
        let n = narrate(&i);
        assert!(
            n.decision("cache")
                .unwrap()
                .value
                .contains("disk slots n/a for this model")
        );
    }

    #[test]
    fn mtp_line_appears_only_when_enabled() {
        let (s, r) = (spec(&[]), cache_ram(Some(6144), CacheRamSource::Auto));
        let off = caps();
        assert!(narrate(&inputs(&s, &off, &r)).decision("mtp").is_none());

        let mut on = caps();
        on.mtp = MtpResolution {
            enabled: true,
            draft_n_max: 2,
            draft_p_min: 0.75,
            source: MtpResolutionSource::MtpTag,
        };
        let n = narrate(&inputs(&s, &on, &r));
        let mtp = n.decision("mtp").unwrap();
        assert_eq!(mtp.value, "on, 2 draft tokens");
        assert_eq!(mtp.source.as_deref(), Some("mtp tag"));
    }

    #[test]
    fn flags_line_carries_per_flag_provenance() {
        let (s, r) = (spec(&[]), cache_ram(Some(6144), CacheRamSource::Auto));
        let mut c = caps();
        c.jinja = JinjaResolution {
            enabled: true,
            source: JinjaResolutionSource::AgentTag,
        };
        c.reasoning = ReasoningFormatResolution {
            format: Some("deepseek".to_string()),
            source: ReasoningFormatSource::ReasoningTag,
        };
        let n = narrate(&inputs(&s, &c, &r));
        assert_eq!(
            n.decision("flags").unwrap().value,
            "--jinja (agent tag) \u{b7} --reasoning-format deepseek (reasoning tag)"
        );
    }

    /// The one flag that takes chat completions away has to be visible in the
    /// banner, or a user staring at a 501 has nothing to go on.
    #[test]
    fn flags_line_names_embedding_mode() {
        let (s, r) = (spec(&[]), cache_ram(Some(6144), CacheRamSource::Auto));
        let mut c = caps();
        c.embeddings = true;
        let n = narrate(&inputs(&s, &c, &r));
        assert_eq!(
            n.decision("flags").unwrap().value,
            "--embeddings (embedding tag)"
        );
    }

    /// No flags means no line — an empty one would imply a decision gglib
    /// never took.
    #[test]
    fn flags_line_is_absent_when_gglib_added_no_flags() {
        let (s, c, r) = (
            spec(&[]),
            caps(),
            cache_ram(Some(6144), CacheRamSource::Auto),
        );
        assert!(narrate(&inputs(&s, &c, &r)).decision("flags").is_none());
    }

    #[test]
    fn dialect_line_names_the_parser_for_a_tagged_model() {
        let (s, c, r) = (
            spec(&[FORMAT_QWEN_XML]),
            caps(),
            cache_ram(Some(6144), CacheRamSource::Auto),
        );
        let n = narrate(&inputs(&s, &c, &r));
        let d = n.decision("dialect").unwrap();
        assert_eq!(d.value, "qwen-xml -> OpenAI tool_calls");
    }

    /// Pass-through is still a decision worth stating.
    #[test]
    fn dialect_line_states_pass_through_for_an_untagged_model() {
        let (s, c, r) = (
            spec(&[]),
            caps(),
            cache_ram(Some(6144), CacheRamSource::Auto),
        );
        let n = narrate(&inputs(&s, &c, &r));
        assert_eq!(n.decision("dialect").unwrap().value, "OpenAI tool_calls");
    }

    /// The mission caps the banner at ~12 lines; the decision list is what
    /// drives that, so it is asserted here rather than in the renderer.
    ///
    /// The renderer spends three lines on chrome (blank, headline, blank), so
    /// nine decisions is the hard ceiling and this now sits exactly on it —
    /// ADR 0003's `sampling` row took the last slot. That is deliberate: the
    /// cap stops being notional, and the next decision anyone wants has to
    /// displace one rather than quietly widen the banner.
    #[test]
    fn narration_stays_within_the_banner_budget() {
        let (s, r) = (
            spec(&[FORMAT_QWEN_XML]),
            cache_ram(Some(6144), CacheRamSource::Auto),
        );
        let mut c = caps();
        c.jinja = JinjaResolution {
            enabled: true,
            source: JinjaResolutionSource::AgentTag,
        };
        c.reasoning = ReasoningFormatResolution {
            format: Some("deepseek".to_string()),
            source: ReasoningFormatSource::ReasoningTag,
        };
        c.mtp = MtpResolution {
            enabled: true,
            draft_n_max: 2,
            draft_p_min: 0.75,
            source: MtpResolutionSource::MtpTag,
        };
        let n = narrate(&inputs(&s, &c, &r));
        assert!(
            n.decisions.len() <= 9,
            "{} decisions would overflow the banner",
            n.decisions.len()
        );
    }
}
