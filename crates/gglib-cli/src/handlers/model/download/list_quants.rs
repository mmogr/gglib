//! `gglib model download <repo> --list-quants`: the repository's variants
//! as the daemon lists them for the GUI's table, one line each with its
//! size, its shards, whether it fits this machine, and what it is.
//!
//! The listing is `DownloadOps::get_model_quantizations`, the call
//! `GET /api/hf/quantizations/{repo}` answers with, so the terminal and
//! the browser's table say the same of every variant, the fit included:
//! judged as the download that follows picks one. Nothing here reads a
//! header or sizes a budget.

use std::fmt::Write as _;
use std::sync::Arc;

use anyhow::{Context as _, Result};
use gglib_app_services::types::{HfQuantization, HfQuantizationsResponse};
use gglib_core::download::GgufFileRole;
use gglib_core::ports::huggingface::{HELPER_FILES_REFUSAL, HfClientPort};

use super::companions;
use crate::bootstrap::CliContext;

/// Print the listing of `model_id`, through a Hub client carrying `token`
/// when one is given (a private repository), else this command's own.
pub(super) async fn execute(ctx: &CliContext, model_id: &str, token: Option<&str>) -> Result<()> {
    println!("Finding available GGUF quantizations for {model_id}...");
    let hf: Arc<dyn HfClientPort> = match token {
        Some(token) => Arc::new(gglib_download::cli_exec::hub_client(Some(token.to_owned()))),
        None => Arc::clone(&ctx.hf_client),
    };
    let listing = companions::listing_ops_with(ctx, hf, None)
        .get_model_quantizations(model_id)
        .await
        .with_context(|| format!("Failed to fetch quantizations for {model_id}"))?;
    print!("{}", quant_listing(&listing));
    Ok(())
}

/// A size as this listing prints it: MiB to one decimal, in integers.
fn mib(bytes: u64) -> String {
    const MIB: u64 = 1_048_576;
    let tenths = (bytes * 10 + MIB / 2) / MIB;
    format!("{}.{} MiB", tenths / 10, tenths % 10)
}

/// What follows a variant's size: nothing for a model.
fn role_mark(variant: &HfQuantization) -> &'static str {
    match variant.role {
        GgufFileRole::Sidecar => " [draft or helper: not downloadable on its own]",
        _ => "",
    }
}

/// The listing as text: each variant with its size, shards, fit and role,
/// the projectors apart when the repository has any, and the command that
/// downloads each model.
pub(super) fn quant_listing(listing: &HfQuantizationsResponse) -> String {
    let model_id = &listing.model_id;
    let shown: Vec<&HfQuantization> = listing
        .quantizations
        .iter()
        .filter(|v| v.role != GgufFileRole::Projector)
        .collect();
    if shown.is_empty() {
        return "✗ No GGUF model found in this repository.\n".to_owned();
    }
    let models: Vec<&HfQuantization> = shown
        .iter()
        .copied()
        .filter(|v| v.role == GgufFileRole::Weights)
        .collect();

    let mut out = format!("✓ Found {} variants:\n", shown.len());
    for variant in &shown {
        let _ = write!(out, "  {} ({})", variant.label, mib(variant.size_bytes));
        if variant.is_sharded {
            let _ = write!(out, " ({} shards)", variant.shard_count.unwrap_or(0));
        }
        if let Some(word) = &variant.fit_word {
            let _ = write!(out, " \u{2014} {word}");
        }
        let _ = writeln!(out, "{}", role_mark(variant));
    }

    let fetched = models.first().and_then(|m| m.projector.as_ref());
    if let Some(fetched) = fetched {
        out.push_str(
            "\nProjectors (for image input; a download fetches the largest with the model):\n",
        );
        for projector in listing
            .quantizations
            .iter()
            .filter(|v| v.role == GgufFileRole::Projector)
        {
            let mark = if projector.file_path == fetched.file_path {
                " <- fetched with every variant"
            } else {
                ""
            };
            let _ = writeln!(
                out,
                "  {} ({}){mark}",
                projector.file_path,
                mib(projector.size_bytes)
            );
        }
    }

    if models.is_empty() {
        let _ = writeln!(
            out,
            "\n✗ No model here, only draft or helper files: {HELPER_FILES_REFUSAL}."
        );
        return out;
    }
    out.push_str("\nTo download a specific variant, use:\n");
    for model in &models {
        let _ = writeln!(out, "  gglib model download {model_id} -q {}", model.label);
    }
    out
}

#[cfg(test)]
#[path = "list_quants_tests.rs"]
mod tests;
