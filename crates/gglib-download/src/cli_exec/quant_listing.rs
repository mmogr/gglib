//! The text `--list-quants` prints for a repository.
//!
//! The variants, each with its label, size and shards, a draft or helper
//! marked as one; then the repository's projectors apart, those whose header
//! says so, with the one a download fetches marked.

use std::fmt::Write as _;

use gglib_core::download::GgufFileRole;
use gglib_core::ports::huggingface::{HELPER_FILES_REFUSAL, HfQuantInfo, largest_projector};

/// A size as this listing prints it.
fn mib(bytes: u64) -> String {
    #[allow(clippy::cast_precision_loss)]
    let mib = bytes as f64 / 1_048_576.0;
    format!("{mib:.1} MiB")
}

/// Whether `variant` is a model: its header says nothing else, or was not
/// read.
fn is_model(variant: &HfQuantInfo) -> bool {
    variant.role.is_none_or(GgufFileRole::is_weights)
}

/// What follows a variant's size: nothing for a model.
fn role_mark(variant: &HfQuantInfo) -> &'static str {
    match variant.role {
        Some(GgufFileRole::Sidecar) => " [draft or helper: not downloadable on its own]",
        _ => "",
    }
}

/// The listing of `model_id`: its variants, `variants` with their roles
/// read, its projectors apart when it has any, and the command that
/// downloads each model.
pub(super) fn quant_listing(model_id: &str, variants: &[HfQuantInfo]) -> String {
    let is_projector = |variant: &&HfQuantInfo| variant.role == Some(GgufFileRole::Projector);
    let shown: Vec<&HfQuantInfo> = variants.iter().filter(|v| !is_projector(v)).collect();
    if shown.is_empty() {
        return "✗ No GGUF model found in this repository.\n".to_owned();
    }
    let models: Vec<&HfQuantInfo> = shown.iter().copied().filter(|v| is_model(v)).collect();

    let mut out = format!("✓ Found {} variants:\n", shown.len());
    for variant in &shown {
        let _ = write!(out, "  {} ({})", variant.label, mib(variant.total_size));
        if variant.is_sharded() {
            let _ = write!(out, " ({} shards)", variant.shard_count());
        }
        let _ = writeln!(out, "{}", role_mark(variant));
    }

    let fetched = largest_projector(variants);
    if let Some(fetched) = fetched.filter(|_| !models.is_empty()) {
        out.push_str(
            "\nProjectors (for image input; a download fetches the largest with the model):\n",
        );
        for projector in variants.iter().filter(is_projector) {
            let mark = if std::ptr::eq(projector, fetched) {
                " <- fetched with every variant"
            } else {
                ""
            };
            let path = projector.file_paths.first().map_or("", String::as_str);
            let _ = writeln!(out, "  {path} ({}){mark}", mib(projector.total_size));
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
#[path = "quant_listing_tests.rs"]
mod tests;
