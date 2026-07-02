//! Core domain types for downloads.
//!
//! Pure data types with no I/O dependencies.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

/// Canonical identifier for a download.
///
/// Represents a unique download as `model_id:quantization` (or just `model_id` if no quantization).
/// This is the single identifier format used throughout the system.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DownloadId {
    model_id: String,
    quantization: Option<String>,
}

impl DownloadId {
    /// Create a new download ID.
    pub fn new(model_id: impl Into<String>, quantization: Option<impl Into<String>>) -> Self {
        Self {
            model_id: model_id.into(),
            quantization: quantization.map(Into::into),
        }
    }

    /// Create a download ID from `model_id` only (no quantization).
    pub fn from_model(model_id: impl Into<String>) -> Self {
        Self {
            model_id: model_id.into(),
            quantization: None,
        }
    }

    /// Get the model ID (e.g., "unsloth/Llama-3").
    #[must_use]
    pub fn model_id(&self) -> &str {
        &self.model_id
    }

    /// Get the quantization type if specified (e.g., "`Q4_K_M`").
    #[must_use]
    pub fn quantization(&self) -> Option<&str> {
        self.quantization.as_deref()
    }

    /// Check if this ID has a quantization specified.
    #[must_use]
    pub const fn has_quantization(&self) -> bool {
        self.quantization.is_some()
    }

    /// Convert to the canonical string format.
    #[must_use]
    pub fn as_canonical(&self) -> String {
        self.to_string()
    }
}

impl fmt::Display for DownloadId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.quantization {
            Some(q) => write!(f, "{}:{q}", self.model_id),
            None => write!(f, "{}", self.model_id),
        }
    }
}

impl FromStr for DownloadId {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // Find the LAST colon that's not part of the model_id
        if let Some(colon_pos) = s.rfind(':') {
            let (model, quant) = s.split_at(colon_pos);
            let quant = &quant[1..]; // Skip the colon

            // Only treat as quantization if it looks like one (no slashes)
            if !quant.is_empty() && !quant.contains('/') {
                return Ok(Self {
                    model_id: model.to_string(),
                    quantization: Some(quant.to_string()),
                });
            }
        }

        Ok(Self {
            model_id: s.to_string(),
            quantization: None,
        })
    }
}

/// Represents the quantization type of a GGUF model file.
///
/// This enum provides type-safe handling of quantization types commonly used
/// in GGUF model naming conventions.
///
/// # Unsloth Dynamic ("UD-") quants
///
/// Unsloth publishes many repos with both a plain quant (e.g. `Q6_K`) and a
/// "UD-" prefixed *Unsloth Dynamic* quant (e.g. `UD-Q6_K`) that otherwise shares
/// the exact same bit-depth suffix. These are modeled as distinct variants
/// (`Q6K` vs `UdQ6K`) so they are never conflated: without this distinction, a
/// naive filename match would treat `Q6_K/model.gguf` and `UD-Q6_K/model.gguf`
/// as the same quantization, which previously caused both to be downloaded
/// together as if they were shards of one request. See [`from_filename`] for
/// how the "UD-" modifier is detected.
///
/// [`from_filename`]: Self::from_filename
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Quantization {
    // 1-bit quantizations
    Iq1S,
    Iq1M,
    /// Unsloth Dynamic ("UD-") variant of [`Self::Iq1S`].
    UdIq1S,
    /// Unsloth Dynamic ("UD-") variant of [`Self::Iq1M`].
    UdIq1M,
    // Ternary quantizations
    Tq1_0,
    /// Unsloth Dynamic ("UD-") variant of [`Self::Tq1_0`].
    UdTq1_0,
    // 2-bit quantizations
    Iq2Xxs,
    /// Unsloth Dynamic ("UD-") variant of [`Self::Iq2Xxs`].
    UdIq2Xxs,
    Iq2Xs,
    Iq2S,
    Iq2M,
    /// Unsloth Dynamic ("UD-") variant of [`Self::Iq2M`].
    UdIq2M,
    Q2KXl,
    Q2KL,
    Q2K,
    // 3-bit quantizations
    Iq3Xxs,
    /// Unsloth Dynamic ("UD-") variant of [`Self::Iq3Xxs`].
    UdIq3Xxs,
    Iq3Xs,
    Iq3S,
    /// Unsloth Dynamic ("UD-") variant of [`Self::Iq3S`].
    UdIq3S,
    Iq3M,
    Q3KXl,
    Q3KL,
    Q3KM,
    /// Unsloth Dynamic ("UD-") variant of [`Self::Q3KM`].
    UdQ3KM,
    Q3KS,
    /// Unsloth Dynamic ("UD-") variant of [`Self::Q3KS`].
    UdQ3KS,
    // 4-bit quantizations
    Iq4Xs,
    /// Unsloth Dynamic ("UD-") variant of [`Self::Iq4Xs`].
    UdIq4Xs,
    Iq4Nl,
    /// Unsloth Dynamic ("UD-") variant of [`Self::Iq4Nl`].
    UdIq4Nl,
    Q4KXl,
    Q4KL,
    Q4KM,
    /// Unsloth Dynamic ("UD-") variant of [`Self::Q4KM`].
    UdQ4KM,
    Q4KS,
    /// Unsloth Dynamic ("UD-") variant of [`Self::Q4KS`].
    UdQ4KS,
    Q4_1,
    Q4_0,
    Mxfp4,
    Q4,
    // 5-bit quantizations
    Q5KXl,
    Q5KL,
    Q5KM,
    /// Unsloth Dynamic ("UD-") variant of [`Self::Q5KM`].
    UdQ5KM,
    Q5KS,
    /// Unsloth Dynamic ("UD-") variant of [`Self::Q5KS`].
    UdQ5KS,
    Q5_0,
    Q5_1,
    Q5,
    // 6-bit quantizations
    Q6KXl,
    Q6KL,
    Q6K,
    /// Unsloth Dynamic ("UD-") variant of [`Self::Q6K`].
    UdQ6K,
    Q6,
    // 8-bit quantizations
    Q8KXl,
    Q8_0,
    Q8,
    // 16-bit and higher precision
    Bf16,
    F16,
    F32,
    // Special formats
    Imatrix,
    // Unknown
    #[default]
    Unknown,
}

/// Pattern table for quantization extraction, ordered by family for readability.
///
/// Ordering is *not* load-bearing for correctness: [`Quantization::from_filename`]
/// checks every pattern and picks the longest boundary-valid match, so a more
/// specific pattern (e.g. `"Q6_K_XL"`) always wins over a shorter one it contains
/// (e.g. `"Q6_K"`) regardless of table order.
///
/// This table intentionally has no `"UD-"` prefixed entries — that modifier is
/// detected orthogonally in `from_filename` via [`UD_VARIANT_MAP`] instead of
/// duplicating every family's pattern string.
const QUANT_PATTERNS: &[(&str, Quantization)] = &[
    // 1-bit
    ("IQ1_S", Quantization::Iq1S),
    ("IQ1_M", Quantization::Iq1M),
    // Ternary
    ("TQ1_0", Quantization::Tq1_0),
    // 2-bit (most specific first)
    ("IQ2_XXS", Quantization::Iq2Xxs),
    ("IQ2_XS", Quantization::Iq2Xs),
    ("IQ2_S", Quantization::Iq2S),
    ("IQ2_M", Quantization::Iq2M),
    ("Q2_K_XL", Quantization::Q2KXl),
    ("Q2_K_L", Quantization::Q2KL),
    ("Q2_K", Quantization::Q2K),
    // 3-bit
    ("IQ3_XXS", Quantization::Iq3Xxs),
    ("IQ3_XS", Quantization::Iq3Xs),
    ("IQ3_S", Quantization::Iq3S),
    ("IQ3_M", Quantization::Iq3M),
    ("Q3_K_XL", Quantization::Q3KXl),
    ("Q3_K_L", Quantization::Q3KL),
    ("Q3_K_M", Quantization::Q3KM),
    ("Q3_K_S", Quantization::Q3KS),
    // 4-bit
    ("IQ4_XS", Quantization::Iq4Xs),
    ("IQ4_NL", Quantization::Iq4Nl),
    ("Q4_K_XL", Quantization::Q4KXl),
    ("Q4_K_L", Quantization::Q4KL),
    ("Q4_K_M", Quantization::Q4KM),
    ("Q4_K_S", Quantization::Q4KS),
    ("Q4_1", Quantization::Q4_1),
    ("Q4_0", Quantization::Q4_0),
    ("MXFP4", Quantization::Mxfp4),
    ("Q4", Quantization::Q4),
    // 5-bit
    ("Q5_K_XL", Quantization::Q5KXl),
    ("Q5_K_L", Quantization::Q5KL),
    ("Q5_K_M", Quantization::Q5KM),
    ("Q5_K_S", Quantization::Q5KS),
    ("Q5_0", Quantization::Q5_0),
    ("Q5_1", Quantization::Q5_1),
    ("Q5", Quantization::Q5),
    // 6-bit
    ("Q6_K_XL", Quantization::Q6KXl),
    ("Q6_K_L", Quantization::Q6KL),
    ("Q6_K", Quantization::Q6K),
    ("Q6", Quantization::Q6),
    // 8-bit
    ("Q8_K_XL", Quantization::Q8KXl),
    ("Q8_0", Quantization::Q8_0),
    ("Q8", Quantization::Q8),
    // 16-bit and higher
    ("BF16", Quantization::Bf16),
    ("FP16", Quantization::F16),
    ("F16", Quantization::F16),
    ("FP32", Quantization::F32),
    ("F32", Quantization::F32),
    // Special
    ("IMATRIX", Quantization::Imatrix),
];

/// Maps a base quantization to its Unsloth Dynamic ("UD-") counterpart, for the
/// families where Unsloth publishes both a plain and a "UD-" prefixed variant
/// with an otherwise identical suffix (e.g. `Q6_K` vs `UD-Q6_K`).
///
/// Kept as a small lookup table rather than duplicating full pattern strings
/// like `"UD-Q6_K"` in [`QUANT_PATTERNS`], so the "UD-" modifier only needs to be
/// detected once, generically, in [`Quantization::from_filename`].
const UD_VARIANT_MAP: &[(Quantization, Quantization)] = &[
    (Quantization::Iq1S, Quantization::UdIq1S),
    (Quantization::Iq1M, Quantization::UdIq1M),
    (Quantization::Tq1_0, Quantization::UdTq1_0),
    (Quantization::Iq2Xxs, Quantization::UdIq2Xxs),
    (Quantization::Iq2M, Quantization::UdIq2M),
    (Quantization::Iq3Xxs, Quantization::UdIq3Xxs),
    (Quantization::Iq3S, Quantization::UdIq3S),
    (Quantization::Q3KM, Quantization::UdQ3KM),
    (Quantization::Q3KS, Quantization::UdQ3KS),
    (Quantization::Iq4Xs, Quantization::UdIq4Xs),
    (Quantization::Iq4Nl, Quantization::UdIq4Nl),
    (Quantization::Q4KM, Quantization::UdQ4KM),
    (Quantization::Q4KS, Quantization::UdQ4KS),
    (Quantization::Q5KM, Quantization::UdQ5KM),
    (Quantization::Q5KS, Quantization::UdQ5KS),
    (Quantization::Q6K, Quantization::UdQ6K),
];

/// Returns the byte offset of the first ASCII case-insensitive occurrence of
/// `pattern` in `haystack` that is flanked by non-alphanumeric characters (or
/// the start/end of the string).
///
/// This boundary requirement avoids false positives against unrelated
/// alphanumeric substrings inside a model name (e.g. a hypothetical segment
/// like `"Q6King"` no longer falsely matches the `"Q6_K"` pattern). Operates
/// directly on byte slices with no heap allocation and no regex, since this
/// runs on every file encountered while scanning a repository's file listing.
fn find_boundary_match(haystack: &[u8], pattern: &[u8]) -> Option<usize> {
    let plen = pattern.len();
    if plen == 0 || plen > haystack.len() {
        return None;
    }

    haystack.windows(plen).enumerate().find_map(|(start, window)| {
        if !window.eq_ignore_ascii_case(pattern) {
            return None;
        }
        let before_ok = start == 0 || !haystack[start - 1].is_ascii_alphanumeric();
        let after = start + plen;
        let after_ok = after == haystack.len() || !haystack[after].is_ascii_alphanumeric();
        (before_ok && after_ok).then_some(start)
    })
}

/// Returns true if the standalone token immediately preceding `match_start`
/// (delimited on both sides) is `"UD"` (ASCII case-insensitive) — e.g. detects
/// the `UD-` in `"...UD-Q6_K.gguf"` when `match_start` is the offset of `Q6_K`.
fn has_ud_prefix(haystack: &[u8], match_start: usize) -> bool {
    // Need room for at least "UD" plus one delimiter byte before the match.
    let Some(token_start) = match_start.checked_sub(3) else {
        return false;
    };
    let delimiter = haystack[match_start - 1];
    if delimiter.is_ascii_alphanumeric() {
        return false;
    }
    if !haystack[token_start..match_start - 1].eq_ignore_ascii_case(b"UD") {
        return false;
    }
    token_start == 0 || !haystack[token_start - 1].is_ascii_alphanumeric()
}

/// Looks up the Unsloth Dynamic ("UD-") counterpart of a base quantization, if
/// one is defined in [`UD_VARIANT_MAP`].
fn ud_variant_of(base: Quantization) -> Option<Quantization> {
    UD_VARIANT_MAP
        .iter()
        .find(|(candidate, _)| *candidate == base)
        .map(|(_, dynamic)| *dynamic)
}

impl Quantization {
    /// Returns true if this quantization type is unknown.
    #[must_use]
    pub const fn is_unknown(&self) -> bool {
        matches!(self, Self::Unknown)
    }

    /// Extract quantization type from a filename.
    ///
    /// Matching is boundary-aware and allocation-free: a pattern only counts as
    /// a match when it is flanked by non-alphanumeric characters (or the
    /// start/end of the string), which avoids false positives against
    /// unrelated alphanumeric substrings in a model name. When multiple
    /// patterns match, the *longest* one wins, so correctness does not depend
    /// on [`QUANT_PATTERNS`] ordering.
    ///
    /// Unsloth's "UD-" ("Unsloth Dynamic") naming convention is detected as an
    /// orthogonal, delimited token immediately preceding the matched base
    /// pattern (see [`UD_VARIANT_MAP`]), rather than by duplicating full
    /// pattern strings like `"UD-Q6_K"` — this keeps e.g. `Q6_K` and `UD-Q6_K`
    /// as distinct values even though they share the same underlying
    /// bit-depth pattern.
    #[must_use]
    pub fn from_filename(filename: &str) -> Self {
        let bytes = filename.as_bytes();
        let mut best: Option<(usize, Self)> = None;

        for (pattern, quant) in QUANT_PATTERNS {
            let Some(pos) = find_boundary_match(bytes, pattern.as_bytes()) else {
                continue;
            };
            let len = pattern.len();
            if best.is_some_and(|(best_len, _)| best_len >= len) {
                continue;
            }
            let resolved = if has_ud_prefix(bytes, pos) {
                ud_variant_of(*quant).unwrap_or(*quant)
            } else {
                *quant
            };
            best = Some((len, resolved));
        }

        best.map_or(Self::Unknown, |(_, q)| q)
    }

    /// Get the canonical string representation.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Iq1S => "IQ1_S",
            Self::Iq1M => "IQ1_M",
            Self::UdIq1S => "UD-IQ1_S",
            Self::UdIq1M => "UD-IQ1_M",
            Self::Tq1_0 => "TQ1_0",
            Self::UdTq1_0 => "UD-TQ1_0",
            Self::Iq2Xxs => "IQ2_XXS",
            Self::UdIq2Xxs => "UD-IQ2_XXS",
            Self::Iq2Xs => "IQ2_XS",
            Self::Iq2S => "IQ2_S",
            Self::Iq2M => "IQ2_M",
            Self::UdIq2M => "UD-IQ2_M",
            Self::Q2KXl => "Q2_K_XL",
            Self::Q2KL => "Q2_K_L",
            Self::Q2K => "Q2_K",
            Self::Iq3Xxs => "IQ3_XXS",
            Self::UdIq3Xxs => "UD-IQ3_XXS",
            Self::Iq3Xs => "IQ3_XS",
            Self::Iq3S => "IQ3_S",
            Self::UdIq3S => "UD-IQ3_S",
            Self::Iq3M => "IQ3_M",
            Self::Q3KXl => "Q3_K_XL",
            Self::Q3KL => "Q3_K_L",
            Self::Q3KM => "Q3_K_M",
            Self::UdQ3KM => "UD-Q3_K_M",
            Self::Q3KS => "Q3_K_S",
            Self::UdQ3KS => "UD-Q3_K_S",
            Self::Iq4Xs => "IQ4_XS",
            Self::UdIq4Xs => "UD-IQ4_XS",
            Self::Iq4Nl => "IQ4_NL",
            Self::UdIq4Nl => "UD-IQ4_NL",
            Self::Q4KXl => "Q4_K_XL",
            Self::Q4KL => "Q4_K_L",
            Self::Q4KM => "Q4_K_M",
            Self::UdQ4KM => "UD-Q4_K_M",
            Self::Q4KS => "Q4_K_S",
            Self::UdQ4KS => "UD-Q4_K_S",
            Self::Q4_1 => "Q4_1",
            Self::Q4_0 => "Q4_0",
            Self::Mxfp4 => "MXFP4",
            Self::Q4 => "Q4",
            Self::Q5KXl => "Q5_K_XL",
            Self::Q5KL => "Q5_K_L",
            Self::Q5KM => "Q5_K_M",
            Self::UdQ5KM => "UD-Q5_K_M",
            Self::Q5KS => "Q5_K_S",
            Self::UdQ5KS => "UD-Q5_K_S",
            Self::Q5_0 => "Q5_0",
            Self::Q5_1 => "Q5_1",
            Self::Q5 => "Q5",
            Self::Q6KXl => "Q6_K_XL",
            Self::Q6KL => "Q6_K_L",
            Self::Q6K => "Q6_K",
            Self::UdQ6K => "UD-Q6_K",
            Self::Q6 => "Q6",
            Self::Q8KXl => "Q8_K_XL",
            Self::Q8_0 => "Q8_0",
            Self::Q8 => "Q8",
            Self::Bf16 => "BF16",
            Self::F16 => "F16",
            Self::F32 => "F32",
            Self::Imatrix => "imatrix",
            Self::Unknown => "unknown",
        }
    }
}

impl fmt::Display for Quantization {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl FromStr for Quantization {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let upper = s.to_uppercase();

        if let Some((_, q)) = QUANT_PATTERNS.iter().find(|(pattern, _)| *pattern == upper) {
            return Ok(*q);
        }

        // Fall back to stripping a "UD-" (Unsloth Dynamic) modifier prefix and
        // mapping the remaining base pattern to its distinct dynamic variant.
        let rest = upper
            .strip_prefix("UD-")
            .or_else(|| upper.strip_prefix("UD_"))
            .ok_or(())?;

        QUANT_PATTERNS
            .iter()
            .find(|(pattern, _)| *pattern == rest)
            .and_then(|(_, base)| ud_variant_of(*base))
            .ok_or(())
    }
}

/// Information about a shard within a sharded model download.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShardInfo {
    /// 0-based index of this shard.
    pub shard_index: u32,
    /// Total number of shards in this model.
    pub total_shards: u32,
    /// The specific filename for this shard.
    pub filename: String,
    /// Size of this shard file in bytes (if known).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_size: Option<u64>,
}

impl ShardInfo {
    /// Create a new `ShardInfo` instance.
    #[must_use]
    pub fn new(shard_index: u32, total_shards: u32, filename: impl Into<String>) -> Self {
        Self {
            shard_index,
            total_shards,
            filename: filename.into(),
            file_size: None,
        }
    }

    /// Create a new `ShardInfo` instance with file size.
    #[must_use]
    pub fn with_size(
        shard_index: u32,
        total_shards: u32,
        filename: impl Into<String>,
        file_size: u64,
    ) -> Self {
        Self {
            shard_index,
            total_shards,
            filename: filename.into(),
            file_size: Some(file_size),
        }
    }

    /// Format as display string (e.g., "Part 1/3").
    #[must_use]
    pub fn display(&self) -> String {
        format!("Part {}/{}", self.shard_index + 1, self.total_shards)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_download_id_display() {
        let id = DownloadId::new("unsloth/Llama-3", Some("Q4_K_M"));
        assert_eq!(id.to_string(), "unsloth/Llama-3:Q4_K_M");

        let id_no_quant = DownloadId::from_model("owner/repo");
        assert_eq!(id_no_quant.to_string(), "owner/repo");
    }

    #[test]
    fn test_download_id_parse() {
        let id: DownloadId = "unsloth/Llama-3:Q4_K_M".parse().unwrap();
        assert_eq!(id.model_id(), "unsloth/Llama-3");
        assert_eq!(id.quantization(), Some("Q4_K_M"));

        let id_no_quant: DownloadId = "owner/repo".parse().unwrap();
        assert_eq!(id_no_quant.model_id(), "owner/repo");
        assert_eq!(id_no_quant.quantization(), None);
    }

    #[test]
    fn test_quantization_from_filename() {
        assert_eq!(
            Quantization::from_filename("model-Q4_K_M.gguf"),
            Quantization::Q4KM
        );
        assert_eq!(
            Quantization::from_filename("llama-F16.gguf"),
            Quantization::F16
        );
        assert_eq!(
            Quantization::from_filename("unknown.gguf"),
            Quantization::Unknown
        );
    }

    #[test]
    fn test_quantization_as_str() {
        assert_eq!(Quantization::Q4KM.as_str(), "Q4_K_M");
        assert_eq!(Quantization::F16.as_str(), "F16");
    }

    #[test]
    fn test_shard_info_display() {
        let shard = ShardInfo::new(1, 5, "model-00002-of-00005.gguf");
        assert_eq!(shard.display(), "Part 2/5");
    }
}
