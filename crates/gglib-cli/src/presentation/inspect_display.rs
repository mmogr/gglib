//! Terminal formatter for `gglib model inspect`.
//!
//! All rendering logic lives here; the handler in
//! `handlers/model/inspect.rs` is kept thin — it only fetches the model,
//! branches on `--json`, and delegates to [`print_model_detail`].

use gglib_core::ModelCapabilities;
use gglib_core::domain::{DefaultsOrigin, MODEL_SAMPLING_KEYS, ModelDetailDto};

use crate::presentation::{first_chars, format_relative_time, print_separator};

const SEP_WIDTH: usize = 60;

/// Render all sections for the given [`ModelDetailDto`] to stdout.
///
/// The `show_metadata` flag gates the raw GGUF key-value section.  Pass
/// `true` only when the user supplies `--metadata` — the dictionary can be
/// several hundred lines for large models.
pub(crate) fn print_model_detail(dto: &ModelDetailDto, show_metadata: bool) {
    // ── Overview ──────────────────────────────────────────────────────────────
    print_separator(SEP_WIDTH);
    println!("  Model: {}", dto.name);
    print_separator(SEP_WIDTH);
    println!("  ID             : {}", dto.id);
    print_opt("  File          ", dto.file_path.as_deref());
    println!("{}", projector_line(dto));
    println!("  Parameters     : {:.1}B", dto.param_count_b);
    if let Some(arch) = &dto.architecture {
        println!("  Architecture   : {arch}");
    }
    if let Some(quant) = &dto.quantization {
        println!("  Quantization   : {quant}");
    }
    if let Some(ctx) = dto.context_length {
        println!("  Context Length : {ctx} tokens");
    }
    if dto.is_serving {
        let port_str = dto.port.map(|p| format!(" (port {p})")).unwrap_or_default();
        println!("  Serving        : yes{port_str}");
    }

    // ── MoE Topology (MoE models only) ────────────────────────────────────────
    if dto.expert_count.is_some() {
        println!();
        println!("  MoE Topology");
        print_separator(SEP_WIDTH);
        if let Some(n) = dto.expert_count {
            println!("  Total Experts  : {n}");
        }
        if let Some(n) = dto.expert_used_count {
            println!("  Used / Token   : {n}");
        }
        if let Some(n) = dto.expert_shared_count {
            println!("  Shared Experts : {n}");
        }
    }

    // ── HuggingFace Provenance ─────────────────────────────────────────────────
    if dto.hf_repo_id.is_some() {
        println!();
        println!("  HuggingFace");
        print_separator(SEP_WIDTH);
        if let Some(repo) = &dto.hf_repo_id {
            println!("  Repo           : {repo}");
        }
        if let Some(filename) = &dto.hf_filename {
            println!("  Filename       : {filename}");
        }
        if let Some(sha) = &dto.hf_commit_sha {
            // Show first 12 chars — enough to identify, not overwhelming.
            println!("  Commit SHA     : {}", first_chars(sha, 12));
        }
        if let Some(dl) = &dto.download_date {
            println!("  Downloaded     : {dl} ({})", format_relative_time(dl));
        }
        if let Some(upd) = &dto.last_update_check {
            println!("  Update Check   : {upd} ({})", format_relative_time(upd));
        }
    }

    // ── Tags ──────────────────────────────────────────────────────────────────
    if !dto.tags.is_empty() {
        println!();
        println!("  Tags");
        print_separator(SEP_WIDTH);
        println!("  {}", dto.tags.join(", "));
    }

    // ── Capabilities ──────────────────────────────────────────────────────────
    println!();
    println!("  Capabilities");
    print_separator(SEP_WIDTH);
    let caps = dto.capabilities;
    println!(
        "  supports-system-role  : {}",
        flag_str(caps.contains(ModelCapabilities::SUPPORTS_SYSTEM_ROLE))
    );
    println!(
        "  requires-strict-turns : {}",
        flag_str(caps.contains(ModelCapabilities::REQUIRES_STRICT_TURNS))
    );
    println!(
        "  supports-tool-calls   : {}",
        flag_str(caps.contains(ModelCapabilities::SUPPORTS_TOOL_CALLS))
    );
    println!(
        "  supports-reasoning    : {}",
        flag_str(caps.contains(ModelCapabilities::SUPPORTS_REASONING))
    );

    // ── Inference Defaults ────────────────────────────────────────────────────
    if let Some(inf) = &dto.inference_defaults {
        let has_any = inf.temperature.is_some()
            || inf.top_p.is_some()
            || inf.top_k.is_some()
            || inf.max_tokens.is_some()
            || inf.repeat_penalty.is_some()
            || inf.presence_penalty.is_some()
            || inf.min_p.is_some()
            || inf.dry_multiplier.is_some()
            || inf.dry_base.is_some()
            || inf.dry_allowed_length.is_some()
            || inf.dry_penalty_last_n.is_some();

        if has_any {
            let origin_suffix = match dto.defaults_origin {
                Some(DefaultsOrigin::AutoDetected) => {
                    " (auto-detected — ranks below global settings)"
                }
                // Same rank as auto-detected, and said so: neither was
                // reviewed by a person, so neither may outrank a setting
                // somebody chose. What differs is the evidence behind it.
                Some(DefaultsOrigin::Published) => {
                    " (published by the model author — ranks below global settings)"
                }
                // Also below global — an automated apply is not a person —
                // but the strongest evidence of the three, and the agentic
                // ceiling defers to it.
                Some(DefaultsOrigin::Measured) => {
                    " (measured by a tune sweep — ranks below global settings)"
                }
                Some(DefaultsOrigin::User) => " (user-set)",
                None => "",
            };
            println!();
            println!("  Inference Defaults{origin_suffix}");
            print_separator(SEP_WIDTH);
            print_opt("  temperature      ", inf.temperature);
            print_opt("  top_p            ", inf.top_p);
            print_opt("  top_k            ", inf.top_k);
            print_opt("  max_tokens       ", inf.max_tokens);
            print_opt("  repeat_penalty   ", inf.repeat_penalty);
            print_opt("  presence_penalty ", inf.presence_penalty);
            print_opt("  min_p            ", inf.min_p);
            print_opt("  dry_multiplier   ", inf.dry_multiplier);
            print_opt("  dry_base         ", inf.dry_base);
            print_opt("  dry_allowed_len  ", inf.dry_allowed_length);
            print_opt("  dry_penalty_last ", inf.dry_penalty_last_n);
        }
    }

    // ── Published Sampling Defaults ───────────────────────────────────────────
    for line in published_sampling_lines(&dto.metadata) {
        println!("{line}");
    }

    // ── Timestamps ────────────────────────────────────────────────────────────
    println!();
    println!("  Timestamps");
    print_separator(SEP_WIDTH);
    println!(
        "  Added          : {} ({})",
        dto.added_at,
        format_relative_time(&dto.added_at)
    );

    // ── Raw GGUF Metadata ─────────────────────────────────────────────────────
    if show_metadata && !dto.metadata.is_empty() {
        println!();
        println!("  Raw GGUF Metadata  ({} keys)", dto.metadata.len());
        print_separator(SEP_WIDTH);
        let mut pairs: Vec<_> = dto.metadata.iter().collect();
        pairs.sort_by_key(|(k, _)| k.as_str());
        for (key, value) in pairs {
            println!("  {key} = {value}");
        }
    }

    print_separator(SEP_WIDTH);
}

// ── Published sampling defaults ───────────────────────────────────────────────

/// The GGUF key prefix llama.cpp reads sampler defaults from.
const SAMPLING_PREFIX: &str = "general.sampling.";

/// Render the `general.sampling.*` keys this model carries, if it carries any.
///
/// # Why these are not left to `--metadata`
///
/// Every other key in that dump describes the model. These *change what the
/// server does*: `common_init_sampler_from_model` (llama.cpp #17120)
/// overwrites `params.sampling` from them for every field no CLI flag sets, and
/// gglib passes no sampler flags at all ([ADR 0003]). A key here is therefore
/// the effective default for any parameter gglib leaves unset — which is most
/// of them.
///
/// Behind `--metadata` they would sit in a several-hundred-line dictionary,
/// alphabetically adjacent to `general.quantization_version`, indistinguishable
/// from trivia. So they get their own always-on section.
///
/// This command reports what the *file* says, so it lists all twelve keys
/// llama.cpp reads rather than only the five gglib compares — an unmodelled key
/// still moves sampling, and hiding it here would make it unfindable. Which of
/// them gglib overrides is [`super::explain_display`]'s question, and the
/// pointer at the end says so rather than answering it twice.
///
/// [ADR 0003]: https://github.com/mmogr/gglib/blob/main/docs/adr/0003-defer-sampler-defaults-to-llama-cpp.md
fn published_sampling_lines(metadata: &std::collections::HashMap<String, String>) -> Vec<String> {
    let mut published: Vec<(&String, &String)> = metadata
        .iter()
        .filter(|(k, _)| k.starts_with(SAMPLING_PREFIX))
        .collect();
    if published.is_empty() {
        return Vec::new();
    }
    published.sort_by_key(|(k, _)| k.as_str());

    let width = published
        .iter()
        .map(|(k, _)| k.chars().count())
        .max()
        .unwrap_or(0);

    let mut lines = vec![
        String::new(),
        "  Published Sampling Defaults  (this model's own GGUF)".to_owned(),
        "-".repeat(SEP_WIDTH),
    ];
    lines.extend(published.iter().map(|(key, value)| {
        // Naming the gglib parameter a key maps onto is the whole reason the
        // reverse lookup exists: `general.sampling.penalty_repeat` and
        // `repeat_penalty` are the same knob under two spellings, and nothing
        // else on screen connects them.
        match gglib_field_for(key) {
            Some(field) => format!("  {key:<width$} = {value}   ({field})"),
            None => format!("  {key:<width$} = {value}   (not modelled by gglib)"),
        }
    }));
    lines.push(String::new());
    lines.push("  llama.cpp applies these to every field gglib does not send.".to_owned());
    lines.push("  Run 'gglib model explain' to see which ones gglib overrides.".to_owned());
    lines
}

/// The gglib parameter a `general.sampling.*` key maps onto, if gglib models
/// it.
///
/// Reverse of [`MODEL_SAMPLING_KEYS`], which is the single mapping table — so
/// this cannot name a pairing the resolution and baseline check disagree with.
///
/// [`MODEL_SAMPLING_KEYS`]: gglib_core::domain::MODEL_SAMPLING_KEYS
fn gglib_field_for(key: &str) -> Option<&'static str> {
    MODEL_SAMPLING_KEYS
        .iter()
        .find(|(_, gguf)| *gguf == key)
        .map(|(field, _)| *field)
}

// ── Projector ─────────────────────────────────────────────────────────────────

/// The line that says whether the model reads images, and through which file.
///
/// A model reads images exactly when it is linked to a projector. The paired
/// machine's answer says that much and withholds the path, which is a place on
/// its disk.
fn projector_line(dto: &ModelDetailDto) -> String {
    let link = match (&dto.projector_path, dto.image_input) {
        (Some(path), _) => path.as_str(),
        (None, true) => "linked",
        (None, false) => "none (text only)",
    };
    format!("  Projector      : {link}")
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn flag_str(v: bool) -> &'static str {
    if v { "yes" } else { "no" }
}

fn print_opt(label: &str, value: Option<impl std::fmt::Display>) {
    if let Some(v) = value {
        println!("{label} : {v}");
    }
}

#[cfg(test)]
#[path = "inspect_display_tests.rs"]
mod tests;
