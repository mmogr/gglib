//! MTP (Multi-Token Prediction) argument resolution for llama.cpp launches.
//!
//! Resolves whether to enable `--spec-type draft-mtp` speculative decoding,
//! and with what parameters, given optional explicit user overrides and the
//! model's capability tags.

/// Indicates how the MTP args were resolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MtpResolutionSource {
    /// User explicitly supplied `--mtp-draft-n-max` (n > 0 → enabled, n = 0 → disabled).
    Explicit,
    /// Auto-enabled because the model carries the `"mtp"` capability tag.
    MtpTag,
    /// Not enabled (default — no tag, no explicit flag).
    Default,
}

/// Result of resolving the MTP speculative decoding parameters.
#[derive(Debug, Clone, PartialEq)]
pub struct MtpResolution {
    /// Whether `--spec-type draft-mtp` should be passed to llama-server.
    pub enabled: bool,
    /// Value for `--spec-draft-n-max`.  Only meaningful when `enabled = true`.
    pub draft_n_max: u32,
    /// Value for `--spec-draft-p-min`.  Only meaningful when `enabled = true`.
    pub draft_p_min: f32,
    /// Source of the decision, used for UX/logging.
    pub source: MtpResolutionSource,
}

/// Default number of MTP draft tokens to speculate ahead.
pub const DEFAULT_DRAFT_N_MAX: u32 = 2;
/// Default minimum acceptance probability for MTP draft tokens.
///
/// 0.75 is the Unsloth recommended value and performs well on Apple Silicon.
pub const DEFAULT_DRAFT_P_MIN: f32 = 0.75;

/// Resolve MTP speculative decoding arguments for a llama-server launch.
///
/// Resolution order (highest priority first):
///
/// 1. **Explicit n = 0** — user explicitly disabled MTP.
/// 2. **Explicit n > 0** — user explicitly enabled MTP with a specific token count.
///    `p_min` uses `explicit_p` if provided, otherwise falls back to 0.75.
/// 3. **`"mtp"` tag** present — auto-enable with defaults (`n = 2`, `p_min = 0.75`).
/// 4. **Default** — disabled; no flags are emitted.
///
/// # Arguments
/// * `explicit_n` — User-specified `--spec-draft-n-max` value (`None` = not set).
/// * `explicit_p` — User-specified `--spec-draft-p-min` value (`None` = not set).
/// * `tags`       — Model capability tags from the database (e.g. `["mtp", "agent"]`).
pub fn resolve_mtp_args(
    explicit_n: Option<u32>,
    explicit_p: Option<f32>,
    tags: &[String],
) -> MtpResolution {
    // 1 + 2. Explicit override: n = 0 disables, n > 0 enables.
    if let Some(n) = explicit_n {
        if n == 0 {
            return MtpResolution {
                enabled: false,
                draft_n_max: 0,
                draft_p_min: DEFAULT_DRAFT_P_MIN,
                source: MtpResolutionSource::Explicit,
            };
        }
        return MtpResolution {
            enabled: true,
            draft_n_max: n,
            draft_p_min: explicit_p.unwrap_or(DEFAULT_DRAFT_P_MIN),
            source: MtpResolutionSource::Explicit,
        };
    }

    // 3. Auto-enable via tag.
    if tags.iter().any(|tag| tag.eq_ignore_ascii_case("mtp")) {
        return MtpResolution {
            enabled: true,
            draft_n_max: DEFAULT_DRAFT_N_MAX,
            draft_p_min: explicit_p.unwrap_or(DEFAULT_DRAFT_P_MIN),
            source: MtpResolutionSource::MtpTag,
        };
    }

    // 4. Default: disabled.
    MtpResolution {
        enabled: false,
        draft_n_max: 0,
        draft_p_min: DEFAULT_DRAFT_P_MIN,
        source: MtpResolutionSource::Default,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tags(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| (*s).to_string()).collect()
    }

    // ── Auto-enable (MtpTag) ──────────────────────────────────────────────

    #[test]
    fn auto_enabled_by_mtp_tag() {
        let res = resolve_mtp_args(None, None, &tags(&["mtp"]));
        assert!(res.enabled);
        assert_eq!(res.draft_n_max, DEFAULT_DRAFT_N_MAX);
        assert!((res.draft_p_min - DEFAULT_DRAFT_P_MIN).abs() < f32::EPSILON);
        assert_eq!(res.source, MtpResolutionSource::MtpTag);
    }

    #[test]
    fn auto_enabled_tag_case_insensitive() {
        let res = resolve_mtp_args(None, None, &tags(&["MTP"]));
        assert!(res.enabled);
        assert_eq!(res.source, MtpResolutionSource::MtpTag);
    }

    #[test]
    fn auto_enable_uses_explicit_p_min_when_provided() {
        let res = resolve_mtp_args(None, Some(0.9), &tags(&["mtp"]));
        assert!(res.enabled);
        assert!((res.draft_p_min - 0.9).abs() < f32::EPSILON);
        assert_eq!(res.source, MtpResolutionSource::MtpTag);
    }

    // ── Explicit enable ───────────────────────────────────────────────────

    #[test]
    fn explicit_n_enables_mtp() {
        let res = resolve_mtp_args(Some(4), None, &tags(&[]));
        assert!(res.enabled);
        assert_eq!(res.draft_n_max, 4);
        assert!((res.draft_p_min - DEFAULT_DRAFT_P_MIN).abs() < f32::EPSILON);
        assert_eq!(res.source, MtpResolutionSource::Explicit);
    }

    #[test]
    fn explicit_n_and_p_both_honored() {
        let res = resolve_mtp_args(Some(3), Some(0.6), &tags(&[]));
        assert!(res.enabled);
        assert_eq!(res.draft_n_max, 3);
        assert!((res.draft_p_min - 0.6).abs() < f32::EPSILON);
        assert_eq!(res.source, MtpResolutionSource::Explicit);
    }

    // ── Explicit disable (n = 0) ──────────────────────────────────────────

    #[test]
    fn explicit_zero_disables_mtp() {
        // Even when the model has the mtp tag, explicit n=0 wins.
        let res = resolve_mtp_args(Some(0), None, &tags(&["mtp"]));
        assert!(!res.enabled);
        assert_eq!(res.source, MtpResolutionSource::Explicit);
    }

    // ── Default (no tag, no explicit) ─────────────────────────────────────

    #[test]
    fn default_disabled_when_no_tag() {
        let res = resolve_mtp_args(None, None, &tags(&["agent", "reasoning"]));
        assert!(!res.enabled);
        assert_eq!(res.source, MtpResolutionSource::Default);
    }

    #[test]
    fn empty_tags_disabled() {
        let res = resolve_mtp_args(None, None, &[]);
        assert!(!res.enabled);
        assert_eq!(res.source, MtpResolutionSource::Default);
    }
}
