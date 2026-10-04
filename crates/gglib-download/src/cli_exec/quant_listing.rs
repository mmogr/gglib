//! The text `--list-quants` prints for a repository.
//!
//! The quantizations, each made of weights alone, then the repository's
//! projectors apart, each marked with the quantizations whose download
//! fetches it.

use std::fmt::Write as _;

use gglib_core::Quantization;
use gglib_core::ports::huggingface::{HfFileInfo, HfQuantInfo, projector_fetched_with};

/// A size as this listing prints it.
fn mib(bytes: u64) -> String {
    #[allow(clippy::cast_precision_loss)]
    let mib = bytes as f64 / 1_048_576.0;
    format!("{mib:.1} MiB")
}

/// For each of `projectors`, the names of the quantizations whose download
/// fetches it.
fn fetched_with<'a>(
    quantizations: &'a [HfQuantInfo],
    projectors: &[HfFileInfo],
) -> Vec<Vec<&'a str>> {
    let mut names = vec![Vec::new(); projectors.len()];
    for quant in quantizations {
        let chosen = projector_fetched_with(Quantization::from_filename(&quant.name), projectors);
        if let Some(index) = chosen.and_then(|c| projectors.iter().position(|p| p.path == c.path)) {
            names[index].push(quant.name.as_str());
        }
    }
    names
}

/// The mark after a projector fetched with `names`, of `total` quantizations:
/// nothing when no download fetches it, the names otherwise. The names are
/// not spelled out when they are every quantization, or when `the_rest` says
/// this projector is the one fetched with all that the others' marks do not
/// name.
fn mark(names: &[&str], total: usize, the_rest: bool) -> String {
    if names.is_empty() {
        String::new()
    } else if names.len() == total {
        " <- fetched with every quantization".to_owned()
    } else if the_rest {
        " <- fetched with every other quantization".to_owned()
    } else {
        format!(" <- fetched with {}", names.join(", "))
    }
}

/// The listing of `model_id`: its quantizations, its projectors when it has
/// any, and the command that downloads each quantization.
pub(super) fn quant_listing(
    model_id: &str,
    quantizations: &[HfQuantInfo],
    projectors: &[HfFileInfo],
) -> String {
    if quantizations.is_empty() {
        return "✗ No GGUF files found in this repository.\n".to_owned();
    }

    let mut out = format!("✓ Found {} quantizations:\n", quantizations.len());
    for quant in quantizations {
        let _ = write!(out, "  {} ({})", quant.name, mib(quant.total_size));
        if quant.shard_count > 1 {
            let _ = write!(out, " ({} shards)", quant.shard_count);
        }
        out.push('\n');
    }

    if !projectors.is_empty() {
        out.push_str("\nProjectors (for image input; a download fetches one with the model):\n");
        let names = fetched_with(quantizations, projectors);
        // Whether one projector is fetched with more quantizations than any
        // other
        let most = names.iter().map(Vec::len).max().unwrap_or(0);
        let alone = names.iter().filter(|n| n.len() == most).count() == 1;
        for (projector, names) in projectors.iter().zip(&names) {
            let mark = mark(names, quantizations.len(), alone && names.len() == most);
            let _ = writeln!(out, "  {} ({}){mark}", projector.path, mib(projector.size));
        }
    }

    out.push_str("\nTo download a specific quantization, use:\n");
    for quant in quantizations {
        let _ = writeln!(out, "  gglib model download {model_id} -q {}", quant.name);
    }
    out
}

#[cfg(test)]
#[path = "quant_listing_tests.rs"]
mod tests;
