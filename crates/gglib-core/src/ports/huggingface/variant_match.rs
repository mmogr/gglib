//! Which of a repository's variants a request names: by its label, as
//! `-q`, `repo:label` and `name@label` ask, else by its file name.

use super::types::HfQuantInfo;
use super::variants::{PROJECTOR_TOKEN, variant_name};

/// The characters a request's token is delimited by inside a label. A `_`
/// is not one: it joins the parts of one quant's name, so `Q6_K` is no
/// token of `Q6_K_P`.
const TOKEN_DELIMITERS: [u8; 3] = *b"-./";

/// Whether `byte` delimits a request's token.
fn delimits_a_request(byte: u8) -> bool {
    TOKEN_DELIMITERS.contains(&byte)
}

/// Whether `byte` delimits a token of a file name, as `mmproj` in
/// `mmproj_F16` or `imatrix` in `imatrix_unsloth`.
pub(super) const fn delimits_a_name(byte: u8) -> bool {
    !byte.is_ascii_alphanumeric()
}

/// Why a request matched no single label.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LabelMismatch {
    /// No label is the request or holds it as a token.
    None,
    /// Several labels hold the request as a token: these.
    Ambiguous(Vec<String>),
}

/// The label of `labels` that `request` names.
///
/// That is the one equal to it ignoring case, else the one label that holds
/// it as a whole token, delimited by `-`, `.`, `/` or an end (`Q4_K_XL`
/// names `UD-Q4_K_XL`; `Q6_K` does not name `Q6_K_P`). Where a model's
/// label and a projector's both hold it (`X.Q8_0` and `X.mmproj-Q8_0`), a
/// request that does not say `mmproj` names the model's.
///
/// # Errors
///
/// [`LabelMismatch::None`] when no label is or holds the request, and
/// [`LabelMismatch::Ambiguous`] with the labels when several hold it.
pub fn match_label<'a>(
    labels: impl IntoIterator<Item = &'a str>,
    request: &str,
) -> Result<&'a str, LabelMismatch> {
    let labels: Vec<&'a str> = labels.into_iter().collect();
    let distinct = |mut found: Vec<&'a str>| {
        found.sort_unstable();
        found.dedup();
        found
    };

    let equal = distinct(
        labels
            .iter()
            .copied()
            .filter(|label| label.eq_ignore_ascii_case(request))
            .collect(),
    );
    match equal.as_slice() {
        [one] => return Ok(one),
        [] => {}
        several => {
            return several
                .iter()
                .find(|label| **label == request)
                .copied()
                .ok_or_else(|| {
                    LabelMismatch::Ambiguous(several.iter().map(|l| (*l).to_owned()).collect())
                });
        }
    }

    let mut holding = distinct(
        labels
            .into_iter()
            .filter(|label| holds_token(label, request, delimits_a_request))
            .collect(),
    );
    let projector = |label: &str| holds_token(label, PROJECTOR_TOKEN, delimits_a_name);
    if holding.len() > 1 && !projector(request) {
        let models: Vec<&'a str> = holding
            .iter()
            .copied()
            .filter(|label| !projector(label))
            .collect();
        if let [model] = models.as_slice() {
            holding = vec![model];
        }
    }
    match holding.as_slice() {
        [one] => Ok(one),
        [] => Err(LabelMismatch::None),
        several => Err(LabelMismatch::Ambiguous(
            several.iter().map(|l| (*l).to_owned()).collect(),
        )),
    }
}

/// The variant of `variants` that `request` names.
///
/// That is the one whose label [`match_label`] reads it as naming, else,
/// when it names no label, the one whose file name holds it as a whole
/// token or ends with it after a byte that is not a letter or digit
/// (`Q4_K_M` names `my_model_Q4_K_M`, `Q6_K` still not `model_Q6_K_P`).
///
/// # Errors
///
/// As [`match_label`]'s, the file names' when they are asked.
pub fn match_variant<'a>(
    variants: impl IntoIterator<Item = &'a HfQuantInfo>,
    request: &str,
) -> Result<&'a HfQuantInfo, LabelMismatch> {
    let variants: Vec<&'a HfQuantInfo> = variants.into_iter().collect();
    match match_label(variants.iter().map(|v| v.label.as_str()), request) {
        Ok(label) => variants
            .iter()
            .copied()
            .find(|v| v.label == label)
            .ok_or(LabelMismatch::None),
        Err(LabelMismatch::None) => {
            let named: Vec<&'a HfQuantInfo> = variants
                .into_iter()
                .filter(|v| {
                    let name = variant_name(v);
                    holds_token(name, request, delimits_a_request) || ends_with_token(name, request)
                })
                .collect();
            match named.as_slice() {
                [one] => Ok(one),
                [] => Err(LabelMismatch::None),
                several => Err(LabelMismatch::Ambiguous(
                    several.iter().map(|v| v.label.clone()).collect(),
                )),
            }
        }
        Err(ambiguous) => Err(ambiguous),
    }
}

/// Whether `name` ends with `token`, ignoring case, after its start or a
/// byte that is not a letter or digit.
fn ends_with_token(name: &str, token: &str) -> bool {
    let (name, token) = (name.as_bytes(), token.as_bytes());
    let Some(start) = name.len().checked_sub(token.len()) else {
        return false;
    };
    !token.is_empty()
        && name[start..].eq_ignore_ascii_case(token)
        && start
            .checked_sub(1)
            .is_none_or(|before| delimits_a_name(name[before]))
}

/// Whether `label` holds `token`, ignoring case, with an end or a byte
/// `delimits` says delimits on either side of it.
pub(super) fn holds_token(label: &str, token: &str, delimits: impl Fn(u8) -> bool) -> bool {
    let (label, token) = (label.as_bytes(), token.as_bytes());
    if token.is_empty() || token.len() > label.len() {
        return false;
    }
    let delimited = |at: Option<&u8>| at.is_none_or(|byte| delimits(*byte));
    label
        .windows(token.len())
        .enumerate()
        .any(|(start, window)| {
            window.eq_ignore_ascii_case(token)
                && delimited(start.checked_sub(1).and_then(|before| label.get(before)))
                && delimited(label.get(start + token.len()))
        })
}

/// The sentence a request that named no single variant of `model_id` is
/// refused with, listing `labels`.
#[must_use]
pub fn label_refusal(
    model_id: &str,
    request: &str,
    labels: &[&str],
    mismatch: &LabelMismatch,
) -> String {
    match mismatch {
        LabelMismatch::None => format!(
            "No variant of {model_id} is labelled '{request}'. Its variants: {}",
            labels.join(", ")
        ),
        LabelMismatch::Ambiguous(several) => format!(
            "'{request}' names several variants of {model_id}: {}. Ask for one by its label.",
            several.join(", ")
        ),
    }
}
