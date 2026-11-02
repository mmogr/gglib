//! The text `--list-quants` prints for a repository.
//!
//! The variants, each with its label, size and shards, a draft or helper
//! marked as one; then the repository's projectors apart, each marked with
//! the variants whose download fetches it.

use std::fmt::Write as _;

use gglib_core::download::GgufFileRole;
use gglib_core::ports::huggingface::{
    HELPER_FILES_REFUSAL, HfFileInfo, HfQuantInfo, projector_fetched_with,
};

/// A size as this listing prints it.
fn mib(bytes: u64) -> String {
    #[allow(clippy::cast_precision_loss)]
    let mib = bytes as f64 / 1_048_576.0;
    format!("{mib:.1} MiB")
}

/// For each of `projectors`, the labels of the models whose download
/// fetches it.
fn fetched_with<'a>(models: &[&'a HfQuantInfo], projectors: &[HfFileInfo]) -> Vec<Vec<&'a str>> {
    let mut names = vec![Vec::new(); projectors.len()];
    for model in models {
        let chosen = projector_fetched_with(&model.label, projectors);
        if let Some(index) = chosen.and_then(|c| projectors.iter().position(|p| p.path == c.path)) {
            names[index].push(model.label.as_str());
        }
    }
    names
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
        Some(GgufFileRole::Projector) => " [projector]",
        _ => "",
    }
}

/// The mark after a projector fetched with `names`, of `total` models:
/// nothing when no download fetches it, the names otherwise. The names are
/// not spelled out when they are every quantization, or when `the_rest` says
/// this projector is the one fetched with all that the others' marks do not
/// name.
fn mark(names: &[&str], total: usize, the_rest: bool) -> String {
    if names.is_empty() {
        String::new()
    } else if names.len() == total {
        " <- fetched with every variant".to_owned()
    } else if the_rest {
        " <- fetched with every other variant".to_owned()
    } else {
        format!(" <- fetched with {}", names.join(", "))
    }
}

/// The listing of `model_id`: its variants, `variants` with their roles
/// read, its projectors when it has any, and the command that downloads
/// each model. A variant whose header says it is a projector and that
/// `projectors` holds is listed with them alone.
pub(super) fn quant_listing(
    model_id: &str,
    variants: &[HfQuantInfo],
    projectors: &[HfFileInfo],
) -> String {
    let listed_apart = |variant: &HfQuantInfo| {
        variant.role == Some(GgufFileRole::Projector)
            && variant
                .file_paths
                .first()
                .is_some_and(|first| projectors.iter().any(|p| &p.path == first))
    };
    let shown: Vec<&HfQuantInfo> = variants.iter().filter(|v| !listed_apart(v)).collect();
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

    if !projectors.is_empty() && !models.is_empty() {
        out.push_str("\nProjectors (for image input; a download fetches one with the model):\n");
        let names = fetched_with(&models, projectors);
        // Whether one projector is fetched with more models than any other
        let most = names.iter().map(Vec::len).max().unwrap_or(0);
        let alone = names.iter().filter(|n| n.len() == most).count() == 1;
        for (projector, names) in projectors.iter().zip(&names) {
            let mark = mark(names, models.len(), alone && names.len() == most);
            let _ = writeln!(out, "  {} ({}){mark}", projector.path, mib(projector.size));
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
