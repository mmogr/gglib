//! A repository's parameter count, and whether a count is its model's.
//!
//! The Hub reads a repository's `gguf` block (`total`, `context_length`)
//! from one file it picks, and it can pick a draft or an MTP head: it
//! answers 1.9B for `HauhauCS`'s 27B model, the count of its `FastMTP-32K`
//! sidecar. A model file's name usually says its size (`…-27B-…`), and a
//! count the repository's largest weights could not be stored at is not the
//! model's.

use super::types::HfQuantInfo;
use super::variants::{largest_weights, said_as_a_quantization};

/// The fewest bits a model's weight is stored in: the smallest quants
/// (`IQ1_S`) take about 1.6.
const FEWEST_BITS_PER_WEIGHT: f64 = 1.0;

/// The most bits a model's weight is stored in, its file's metadata
/// counted: a full-precision `F32` file is 32 and a little over. Repositories
/// on the Hub sit between 16 and 33 for their largest file.
const MOST_BITS_PER_WEIGHT: f64 = 34.0;

/// Whether `parameters_b` billion weights fit `weights_bytes`.
///
/// That is, whether they could be the model of a repository whose largest
/// weights variant takes `weights_bytes`: between [`FEWEST_BITS_PER_WEIGHT`]
/// and [`MOST_BITS_PER_WEIGHT`] bits each.
///
/// `HauhauCS`'s `Q8_K_P` of 31.5 GB over the Hub's 1.86B is 135 bits a
/// weight, and is no count of it; over the 27B its name says, 9.3.
#[must_use]
pub fn plausible_total(parameters_b: f64, weights_bytes: u64) -> bool {
    // Whole MiB are exact enough for a ratio judged to a bit.
    let mib = u32::try_from(weights_bytes >> 20).unwrap_or(u32::MAX);
    let bits = f64::from(mib) * f64::from(1_u32 << 20) * 8.0;
    parameters_b > 0.0
        && (FEWEST_BITS_PER_WEIGHT..=MOST_BITS_PER_WEIGHT).contains(&(bits / (parameters_b * 1e9)))
}

/// The parameter count, in billions, a model file's name states.
///
/// That is its first token, between `-` and `_`, that reads `<n>B` or `<n>M`
/// (`27B`, `3.8B`, `270m`), a million a thousandth of a billion. `None` when
/// no token does, and for a mixture of experts' `NxM` label (`8x7B`), which
/// states no total.
///
/// Tokens that only hold a number (`2511`, `BF16`, `A3B`) state none, so
/// `qwen-image-edit-2511-BF16` gives no count rather than 2511B.
#[must_use]
pub fn named_parameters_b(file_name: &str) -> Option<f64> {
    let name = file_name.rsplit('/').next().unwrap_or(file_name);
    let stem = name
        .len()
        .checked_sub(".gguf".len())
        .filter(|&at| name.is_char_boundary(at) && name[at..].eq_ignore_ascii_case(".gguf"))
        .map_or(name, |at| &name[..at]);
    for token in stem.split(['-', '_']) {
        if experts_label(token) {
            return None;
        }
        if let Some(count) = stated_count(token) {
            return Some(count);
        }
    }
    None
}

/// The largest count any of `variants`' model files states by its name
/// ([`named_parameters_b`]); a projector's or an importance matrix's is not
/// read.
#[must_use]
pub fn largest_named(variants: &[HfQuantInfo]) -> Option<f64> {
    variants
        .iter()
        .filter(|variant| said_as_a_quantization(variant))
        .filter_map(|variant| named_parameters_b(variant.file_paths.first()?))
        .max_by(f64::total_cmp)
}

/// Whether the names of `variants`' model files contradict `hub_b`.
///
/// They do when one states more than twice the Hub's count, or states a
/// count where the Hub gives none. Read from names alone, so a search can
/// tell before it lists a hit's files.
#[must_use]
pub fn names_contradict(hub_b: Option<f64>, variants: &[HfQuantInfo]) -> bool {
    largest_named(variants).is_some_and(|named| hub_b.is_none_or(|hub| named > 2.0 * hub))
}

/// The parameter count of a repository whose sized files are `variants`.
///
/// The Hub counted it as `hub_b`, which stands when the largest weights'
/// size is unknown (0). `None` when it has no weights to judge by, or
/// neither count could be held by them.
///
/// The count its largest weights' name states (else the largest any model
/// file's states) when it contradicts the Hub's ([`names_contradict`]), else
/// the Hub's, then the other; each only when those weights could hold it
/// ([`plausible_total`]). A name that agrees with the Hub gives way to it:
/// the Hub's 3.88B for a file named `4b`.
#[must_use]
pub fn sized_parameters_b(hub_b: Option<f64>, variants: &[HfQuantInfo]) -> Option<f64> {
    let weights = largest_weights(variants)?;
    if weights.total_size == 0 {
        // A listing that gave no size judges nothing: the Hub's stands.
        return hub_b;
    }
    let holds = |count: &f64| plausible_total(*count, weights.total_size);
    let named = weights
        .file_paths
        .first()
        .and_then(|path| named_parameters_b(path))
        .or_else(|| largest_named(variants));
    let (first, then) = if names_contradict(hub_b, variants) {
        (named, hub_b)
    } else {
        (hub_b, named)
    };
    first.filter(holds).or_else(|| then.filter(holds))
}

/// The count `token` states as `<n>B` or `<n>M`, `<n>` digits with at most
/// one `.`, in billions.
fn stated_count(token: &str) -> Option<f64> {
    let (number, unit) = token.split_at(token.len().checked_sub(1)?);
    let scale = match unit {
        "B" | "b" => 1.0,
        "M" | "m" => 0.001,
        _ => return None,
    };
    let (whole, fraction) = number.split_once('.').unwrap_or((number, "0"));
    let digits = |part: &str| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit());
    if !digits(whole) || !digits(fraction) {
        return None;
    }
    number
        .parse::<f64>()
        .ok()
        .map(|count| count * scale)
        .filter(|count| *count > 0.0)
}

/// Whether `token` is a mixture of experts' `NxM` label (`8x7B`, `512x56B`).
fn experts_label(token: &str) -> bool {
    token.split_once(['x', 'X']).is_some_and(|(experts, each)| {
        !experts.is_empty()
            && experts.bytes().all(|b| b.is_ascii_digit())
            && stated_count(each).is_some()
    })
}

#[cfg(test)]
#[path = "hub_count_tests.rs"]
mod tests;
