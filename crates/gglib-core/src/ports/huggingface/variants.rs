//! A repository's downloadable variants: its GGUF files grouped into shard
//! sets, each named as its uploader named it.
//!
//! A variant is the files whose path, less llama.cpp's shard suffix
//! (`-00001-of-00003`) and `.gguf`, is the same. Nothing is read from a name
//! but that: the label is the uploader's own word for the variant, whatever
//! quant it states, and what the variant *is* comes from its first file's
//! header ([`super::variant_role`]).

use std::collections::{BTreeMap, HashMap};

use super::types::HfQuantInfo;
use super::variant_match::{delimits_a_name, holds_token};
use crate::domain::shard_name::{gguf_stem, shard_parts};

/// Why a draft or helper variant is not downloaded as a model.
pub const HELPER_FILES_REFUSAL: &str = "gglib can't run draft or helper files on their own";

/// The characters a label may not hold, each written as `-`: `DownloadId`
/// splits on its last `:`, a repository path on `/`, and a listed id on `@`.
const RESERVED: [char; 3] = [':', '@', '/'];

/// The token a projector's file name carries.
pub(super) const PROJECTOR_TOKEN: &str = "mmproj";

/// The token an importance matrix's file name carries.
const IMATRIX_TOKEN: &str = "imatrix";

/// The tokens a helper's file name carries: an importance matrix's, and an
/// MTP head's.
const HELPER_TOKENS: [&str; 2] = [IMATRIX_TOKEN, "mtp"];

/// Whether `variant` is said as a quantisation where a search lists them.
///
/// That is when its file name holds no projector's token (`mmproj`) and no
/// importance matrix's (`imatrix`). It is read from the name for that line
/// alone; a draft or MTP head is said, as a listing dims it. What a variant
/// is, which a download and a listing go by, comes from its header
/// ([`super::variant_role`]).
#[must_use]
pub fn said_as_a_quantization(variant: &HfQuantInfo) -> bool {
    let name = variant_name(variant);
    ![PROJECTOR_TOKEN, IMATRIX_TOKEN]
        .iter()
        .any(|token| holds_token(name, token, delimits_a_name))
}

/// `files`, each a path and a size, grouped into the repository's variants
/// and labelled, by label. A file that is not a `.gguf` is no variant's.
///
/// Each variant's role is `None`: the listing reads no header.
///
/// The label is the variant's file name less the prefix the names share,
/// cut back to a separator ([`shared_prefix`]), so `model-Q4_K_M` and
/// `model-Q4_K_S` share `model-` and are labelled `Q4_K_M` and `Q4_K_S`. A
/// name the prefix would leave empty, or that does not start with it, is its
/// own label. The prefix is read from every name but a projector's
/// (`mmproj`), and but a helper's (`imatrix`, `mtp`) while two others are
/// left: that is all a label takes from a name's tokens, and a variant's
/// role comes from its header. A name that several folders hold is labelled
/// by its folder, and a repository with one name to read a prefix from
/// labels every variant by its whole name. A token of a label that spells a
/// quantisation is written in capitals (`Q6_k` is `Q6_K`), so a variant
/// reads one way whoever uploaded it; a request still matches it in any
/// case.
pub fn variants_of<'a>(files: impl IntoIterator<Item = (&'a str, u64)>) -> Vec<HfQuantInfo> {
    // Keyed by the path less the shard's index and the extension: the
    // shard total stays, so sets of different totals are not one variant.
    let mut groups: BTreeMap<String, Group<'a>> = BTreeMap::new();
    for (path, size) in files {
        if let Some((key, stem)) = variant_key(path) {
            let group = groups.entry(key).or_insert_with(|| (stem, Vec::new()));
            group.1.push((path, size));
        }
    }

    let keys: Vec<&str> = groups.keys().map(String::as_str).collect();
    let stems: Vec<&str> = groups.values().map(|(stem, _)| stem.as_str()).collect();
    let labels = unique_labels(&keys, labels_of(&stems))
        .into_iter()
        .map(|label| upper_quantizations(&label));
    let mut variants: Vec<HfQuantInfo> = groups
        .into_values()
        .zip(labels)
        .map(|((_, mut files), label)| {
            files.sort_unstable_by(|a, b| a.0.cmp(b.0));
            HfQuantInfo {
                label,
                role: None,
                total_size: files.iter().map(|(_, size)| size).sum(),
                file_paths: files.iter().map(|(path, _)| (*path).to_owned()).collect(),
            }
        })
        .collect();
    variants.sort_by(|a, b| a.label.cmp(&b.label));
    variants
}

/// One variant's stem, as [`variant_key`] gives it, and its files, each a
/// path and a size.
type Group<'a> = (String, Vec<(&'a str, u64)>);

/// What a `.gguf` path's variant is keyed by, and its stem: the path less
/// the extension, and less a shard's index (`-00001`) while keeping its
/// total (`-of-00003`) in the key alone. `None` for any other file.
fn variant_key(path: &str) -> Option<(String, String)> {
    let extension = path.len().checked_sub(".gguf".len())?;
    if !path.is_char_boundary(extension) || !path[extension..].eq_ignore_ascii_case(".gguf") {
        return None;
    }
    let stem = &path[..extension];
    Some(match shard_parts(stem) {
        Some((unsharded, total)) => (format!("{unsharded}-of-{total}"), unsharded.to_owned()),
        None => (stem.to_owned(), stem.to_owned()),
    })
}

/// The file name a variant is named by: its first file's, less the
/// extension and a shard's numbering.
pub(super) fn variant_name(variant: &HfQuantInfo) -> &str {
    let first = variant.file_paths.first().map_or("", String::as_str);
    gguf_stem(first.rsplit('/').next().unwrap_or(first))
}

/// `labels`, the label of each of `keys` in their order, with every label
/// that two variants would share, ignoring case, replaced by its variant's
/// whole key, and a number added where even keys read alike once written
/// as labels.
fn unique_labels(keys: &[&str], mut labels: Vec<String>) -> Vec<String> {
    let shared = |labels: &[String], at: usize| {
        labels
            .iter()
            .enumerate()
            .any(|(other, label)| other != at && label.eq_ignore_ascii_case(&labels[at]))
    };
    let clashing: Vec<usize> = (0..labels.len())
        .filter(|&at| shared(&labels, at))
        .collect();
    for at in clashing {
        labels[at] = keys[at].replace(RESERVED, "-");
    }
    for at in 0..labels.len() {
        let mut n = 2;
        let base = labels[at].clone();
        while shared(&labels[..=at], at) {
            labels[at] = format!("{base}-{n}");
            n += 1;
        }
    }
    labels
}

/// The label of each of `stems`, in their order.
fn labels_of(stems: &[&str]) -> Vec<String> {
    let base = |stem: &str| stem.rsplit('/').next().unwrap_or(stem).to_owned();
    let bases: Vec<String> = stems.iter().map(|stem| base(stem)).collect();
    let mut seen: HashMap<&str, usize> = HashMap::new();
    for name in &bases {
        *seen.entry(name.as_str()).or_default() += 1;
    }

    // The names the prefix is read from: a model's, when at least two are
    // named as one; else every name but a projector's.
    let names_a = |name: &str, token: &str| holds_token(name, token, delimits_a_name);
    let not_projectors: Vec<&str> = bases
        .iter()
        .map(String::as_str)
        .filter(|name| !names_a(name, PROJECTOR_TOKEN))
        .collect();
    let models: Vec<&str> = not_projectors
        .iter()
        .copied()
        .filter(|name| !HELPER_TOKENS.iter().any(|token| names_a(name, token)))
        .collect();
    let named = if models.len() > 1 {
        models
    } else {
        not_projectors
    };
    let prefix = if named.len() > 1 {
        shared_prefix(&named)
    } else {
        ""
    };

    stems
        .iter()
        .zip(&bases)
        .map(|(stem, name)| {
            let folder = stem.rsplit_once('/').map(|(folder, _)| folder);
            let label = match folder {
                Some(folder) if seen[name.as_str()] > 1 => folder,
                _ => name
                    .strip_prefix(prefix)
                    .map(|rest| rest.strip_prefix(['-', '.']).unwrap_or(rest))
                    .filter(|rest| !rest.is_empty())
                    .unwrap_or(name),
            };
            label.replace(RESERVED, "-")
        })
        .collect()
}

/// `label` with each token, between `-` and `.`, that spells a
/// quantisation written in capitals, so a variant reads one way whoever
/// uploaded it: `Q6_k` is `Q6_K`, `iq4_xs` is `IQ4_XS` and `bf16` is `BF16`.
/// Every other token is kept as written (`UD`, `FastMTP-32K`, `Thinking`).
fn upper_quantizations(label: &str) -> String {
    let mut written = String::with_capacity(label.len());
    for (at, token) in label.split(['-', '.']).enumerate() {
        if at > 0 {
            // The separator before this token: one byte, `-` or `.`.
            written.push(char::from(label.as_bytes()[written.len()]));
        }
        if spells_a_quantization(token) {
            written.push_str(&token.to_ascii_uppercase());
        } else {
            written.push_str(token);
        }
    }
    written
}

/// Whether `token` spells a quantisation, in any case: `Q`, `IQ` or `TQ`,
/// digits, then parts of one to three letters or digits each after a `_`
/// (`Q4_K_M`, `IQ2_XXS`, `TQ1_0`, `Q8_0`); or `F16`, `BF16`, `F32`.
fn spells_a_quantization(token: &str) -> bool {
    let upper = token.to_ascii_uppercase();
    if matches!(upper.as_str(), "F16" | "BF16" | "F32") {
        return true;
    }
    let Some(rest) = ["IQ", "TQ", "Q"]
        .iter()
        .find_map(|start| upper.strip_prefix(start))
    else {
        return false;
    };
    let mut parts = rest.split('_');
    let bits = parts.next().unwrap_or_default();
    !bits.is_empty()
        && bits.bytes().all(|b| b.is_ascii_digit())
        && parts.all(|part| {
            (1..=3).contains(&part.len()) && part.bytes().all(|b| b.is_ascii_alphanumeric())
        })
}

/// The longest prefix every one of `names` starts with, when each goes on
/// with a `-` or a `.`, which the label then drops (`X-Q2_K` and `X.BF16`
/// share `X`). Otherwise the prefix is cut back to its last `-` or `.`, or,
/// when it holds neither, to its first `_`; empty when it holds none.
///
/// A `_` joins the parts of one quant's name (`Q4_K_M`), so it cuts the
/// prefix only where nothing else can, and then as early as it can:
/// `model-Q4_K_M` and `model-Q4_K_S` share `model-`, `model_Q4_K_M` and
/// `model_Q4_K_S` share `model_`, and `model-Q6_K` and `model-Q6_K_P` share
/// `model-`.
fn shared_prefix<'a>(names: &[&'a str]) -> &'a str {
    let Some((first, rest)) = names.split_first() else {
        return "";
    };
    let mut common = first.len();
    for name in rest {
        common = first
            .char_indices()
            .zip(name.chars())
            .take_while(|((_, a), b)| a == b)
            .last()
            .map_or(0, |((at, a), _)| at + a.len_utf8())
            .min(common);
    }
    let at_a_boundary = common > 0
        && names
            .iter()
            .all(|name| name[common..].starts_with(['-', '.']));
    if at_a_boundary {
        return &first[..common];
    }
    let common = &first[..common];
    common
        .rfind(['-', '.'])
        .or_else(|| common.find('_'))
        .map_or("", |cut| &first[..=cut])
}

#[cfg(test)]
#[path = "variants_tests.rs"]
mod tests;
