//! Terminal formatter for `gglib model inspect`.
//!
//! All rendering logic lives here; the handler in
//! `handlers/model/inspect.rs` is kept thin — it only fetches the model,
//! branches on `--json`, and delegates to [`print_model_detail`].

use gglib_app_services::types::ModelDetailDto;
use gglib_core::ModelCapabilities;

use crate::presentation::{format_relative_time, print_separator};

const SEP_WIDTH: usize = 60;

/// Render all sections for the given [`ModelDetailDto`] to stdout.
///
/// The `show_metadata` flag gates the raw GGUF key-value section.  Pass
/// `true` only when the user supplies `--metadata` — the dictionary can be
/// several hundred lines for large models.
pub fn print_model_detail(dto: &ModelDetailDto, show_metadata: bool) {
    // ── Overview ──────────────────────────────────────────────────────────────
    print_separator(SEP_WIDTH);
    println!("  Model: {}", dto.name);
    print_separator(SEP_WIDTH);
    println!("  ID             : {}", dto.id);
    println!("  File           : {}", dto.file_path);
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
        let port_str = dto
            .port
            .map(|p| format!(" (port {p})"))
            .unwrap_or_default();
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
            println!("  Commit SHA     : {}", &sha[..sha.len().min(12)]);
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
            || inf.min_p.is_some();

        if has_any {
            println!();
            println!("  Inference Defaults");
            print_separator(SEP_WIDTH);
            print_opt_f32("  temperature      ", inf.temperature);
            print_opt_f32("  top_p            ", inf.top_p);
            print_opt_i32("  top_k            ", inf.top_k);
            print_opt_u32("  max_tokens       ", inf.max_tokens);
            print_opt_f32("  repeat_penalty   ", inf.repeat_penalty);
            print_opt_f32("  presence_penalty ", inf.presence_penalty);
            print_opt_f32("  min_p            ", inf.min_p);
        }
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

// ── Helpers ───────────────────────────────────────────────────────────────────

fn flag_str(v: bool) -> &'static str {
    if v { "yes" } else { "no" }
}

fn print_opt_f32(label: &str, value: Option<f32>) {
    if let Some(v) = value {
        println!("{label} : {v}");
    }
}

fn print_opt_i32(label: &str, value: Option<i32>) {
    if let Some(v) = value {
        println!("{label} : {v}");
    }
}

fn print_opt_u32(label: &str, value: Option<u32>) {
    if let Some(v) = value {
        println!("{label} : {v}");
    }
}
