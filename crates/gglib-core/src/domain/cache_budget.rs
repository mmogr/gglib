//! Auto-sizing math for llama-server's host-RAM prompt cache (`--cache-ram`).
//!
//! Extracted from `server_config` so the pure budget arithmetic lives
//! alongside the rest of the domain's pure calculations, with its own
//! focused test suite.

/// RAM reserved for the OS, other applications, and llama.cpp's own
/// compute/scratch buffers — never handed to the prompt cache.
pub const CACHE_RAM_HEADROOM_BYTES: u64 = 16 * 1024 * 1024 * 1024;

/// Below this, a prompt cache holds too little to be worth the memory
/// pressure, so the budget collapses to `0` (explicitly disabled).
pub const CACHE_RAM_FLOOR_BYTES: u64 = 1024 * 1024 * 1024;

/// KV allowance assumed when the model's metadata doesn't permit an estimate.
/// Deliberately generous: over-reserving shrinks the cache (safe), whereas
/// under-reserving risks memory pressure.
pub const CACHE_RAM_UNKNOWN_KV_ALLOWANCE_BYTES: u64 = 8 * 1024 * 1024 * 1024;

/// Threshold below which a working prompt cache counts as cramped.
///
/// At or below this the cache holds too few conversations to reliably survive
/// switching between them, so a resumed conversation will often re-prefill
/// from scratch. Sits above [`CACHE_RAM_FLOOR_BYTES`], so it describes a cache
/// that is working but tight — not one that was switched off.
pub const CACHE_RAM_LOW_WATERMARK_BYTES: u64 = 4 * 1024 * 1024 * 1024;

// The two thresholds must not overlap: if the watermark ever dropped to or
// below the floor, `classify_cache_ram` could never return `Low`, silently
// emptying the warning band. Enforced at compile time rather than in a test,
// since both operands are constants and the mistake would be a source edit.
const _: () = assert!(CACHE_RAM_FLOOR_BYTES < CACHE_RAM_LOW_WATERMARK_BYTES);

/// How healthy a resolved `--cache-ram` budget is, for user-facing display.
///
/// Exists so surfaces (dashboard, CLI) don't re-derive the thresholds from
/// magic numbers. In particular, a `0` budget is genuinely ambiguous at the
/// call site — [`compute_auto_cache_ram_mb`] returns `0` when the machine
/// can't afford a cache, and a user can also pass `--cache-ram-mb 0` — and
/// those need different messages, since only one of them is a problem.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheRamHealth {
    /// No `--cache-ram` flag emitted; llama-server's built-in default applies.
    LlamaDefault,
    /// The user asked for `0`. Working as intended — not a warning.
    DisabledByUser,
    /// Auto-sizing found no room after weights, KV, and headroom. The machine
    /// cannot afford a prompt cache at this model and context size.
    DisabledInsufficientRam,
    /// Working, but at or under [`CACHE_RAM_LOW_WATERMARK_BYTES`] — expect
    /// conversation switches to re-prefill more often than not.
    Low { mb: u64 },
    /// Comfortably sized.
    Healthy { mb: u64 },
}

impl CacheRamHealth {
    /// Whether this state is worth drawing the user's attention to.
    ///
    /// `false` for both healthy budgets and a deliberately disabled one —
    /// warning someone about a setting they chose is noise.
    #[must_use]
    pub const fn needs_attention(&self) -> bool {
        matches!(self, Self::DisabledInsufficientRam | Self::Low { .. })
    }
}

/// Classify a resolved `--cache-ram` budget.
///
/// # Arguments
///
/// * `cache_ram_mb` — the resolved budget, or `None` when no flag is emitted.
/// * `was_explicit` — whether the value came from the user rather than
///   auto-sizing. Only consulted to disambiguate `0`; a small-but-nonzero
///   budget is reported as [`CacheRamHealth::Low`] either way, because the
///   consequence (switches re-prefill) is the same regardless of who chose it.
#[must_use]
pub const fn classify_cache_ram(cache_ram_mb: Option<u64>, was_explicit: bool) -> CacheRamHealth {
    let Some(mb) = cache_ram_mb else {
        return CacheRamHealth::LlamaDefault;
    };
    if mb == 0 {
        return if was_explicit {
            CacheRamHealth::DisabledByUser
        } else {
            CacheRamHealth::DisabledInsufficientRam
        };
    }
    if mb.saturating_mul(1024 * 1024) <= CACHE_RAM_LOW_WATERMARK_BYTES {
        return CacheRamHealth::Low { mb };
    }
    CacheRamHealth::Healthy { mb }
}

/// Compute the auto `--cache-ram` budget, in MiB.
///
/// ```text
/// budget = total_ram − model_weights − kv_bytes − HEADROOM
/// result = if budget < FLOOR { 0 } else { budget }
/// ```
///
/// Claims all RAM safely available after weights, KV, and headroom — no
/// fractional cap. Saturating throughout: a model larger than RAM yields `0`
/// (cache disabled) rather than wrapping into a huge budget.
///
/// # Arguments
///
/// * `total_ram_bytes` — total physical system RAM.
/// * `model_bytes` — on-disk size of the model weights (all shards).
/// * `kv_bytes` — estimated KV cache at the launch context size; pass
///   [`CACHE_RAM_UNKNOWN_KV_ALLOWANCE_BYTES`] when unknown.
#[must_use]
pub const fn compute_auto_cache_ram_mb(
    total_ram_bytes: u64,
    model_bytes: u64,
    kv_bytes: u64,
) -> u64 {
    let reserved = model_bytes
        .saturating_add(kv_bytes)
        .saturating_add(CACHE_RAM_HEADROOM_BYTES);
    let budget = total_ram_bytes.saturating_sub(reserved);
    if budget < CACHE_RAM_FLOOR_BYTES {
        return 0;
    }
    budget / (1024 * 1024)
}

#[cfg(test)]
mod tests {
    use super::*;

    const GIB: u64 = 1024 * 1024 * 1024;

    /// The reference case: 128 GiB machine, 27 GiB weights, ~9 GiB KV.
    /// No fractional cap — the full ~76 GiB remainder is claimed.
    #[test]
    fn auto_budget_claims_all_safely_available_ram() {
        let got = compute_auto_cache_ram_mb(128 * GIB, 27 * GIB, 9 * GIB);
        assert_eq!(got, 76 * 1024);
    }

    /// A large machine where the old 25% cap would have bound (128 GiB) no
    /// longer loses that headroom — the full remainder is claimed.
    #[test]
    fn auto_budget_uncapped_on_a_large_machine() {
        // reserved = 27 + 9 + 16 = 52; usable = 512 - 52 = 460 GiB.
        // The old 25% cap (128 GiB) would have bound here; it no longer does.
        let got = compute_auto_cache_ram_mb(512 * GIB, 27 * GIB, 9 * GIB);
        assert_eq!(got, 460 * 1024);
    }

    /// Straightforward subtraction case, well above the floor.
    #[test]
    fn auto_budget_is_total_minus_reserved() {
        // 64 - 30 - 4 - 16 = 14 GiB usable.
        let got = compute_auto_cache_ram_mb(64 * GIB, 30 * GIB, 4 * GIB);
        assert_eq!(got, 14 * 1024);
    }

    /// A model that leaves under the 1 GiB floor disables the cache outright
    /// rather than letting llama-server apply its 8 GiB default.
    #[test]
    fn auto_budget_collapses_to_zero_under_the_floor() {
        // 36 - 20 - 4 - 16 = saturates to 0.
        assert_eq!(compute_auto_cache_ram_mb(36 * GIB, 20 * GIB, 4 * GIB), 0);
    }

    /// A model larger than total RAM must saturate to 0, never wrap around
    /// into an enormous budget.
    #[test]
    fn auto_budget_saturates_when_model_exceeds_ram() {
        assert_eq!(compute_auto_cache_ram_mb(16 * GIB, 64 * GIB, 8 * GIB), 0);
    }

    /// 8 GiB laptop: headroom (16 GiB) alone exceeds total RAM, so reserved
    /// saturates past the machine's capacity → budget collapses to 0.
    #[test]
    fn auto_budget_is_zero_on_small_ram_laptop() {
        // reserved = 3 + 0 + 16 = 19 > 8 → usable = 0
        assert_eq!(compute_auto_cache_ram_mb(8 * GIB, 3 * GIB, 0), 0);
    }

    /// 24 GiB machine: subtraction lands exactly on the 1 GiB floor.
    #[test]
    fn auto_budget_hits_floor_boundary_at_24_gib() {
        // reserved = 7 + 0 + 16 = 23; usable = 24 - 23 = 1 GiB → 1024 MiB
        assert_eq!(compute_auto_cache_ram_mb(24 * GIB, 7 * GIB, 0), 1024);
    }

    /// 32 GiB machine: comfortably above the floor.
    #[test]
    fn auto_budget_above_floor_on_32_gib_machine() {
        // reserved = 10 + 0 + 16 = 26; usable = 32 - 26 = 6 GiB → 6144 MiB
        assert_eq!(compute_auto_cache_ram_mb(32 * GIB, 10 * GIB, 0), 6144);
    }

    // ── Budget health classification ─────────────────────────────────────

    /// No flag at all is llama-server's own default, not a disabled cache.
    #[test]
    fn classify_none_is_llama_default() {
        assert_eq!(
            classify_cache_ram(None, false),
            CacheRamHealth::LlamaDefault
        );
        assert_eq!(classify_cache_ram(None, true), CacheRamHealth::LlamaDefault);
    }

    /// The whole point of the enum: a `0` the user asked for and a `0` the
    /// machine forced must not read the same to a surface.
    #[test]
    fn classify_distinguishes_chosen_zero_from_forced_zero() {
        assert_eq!(
            classify_cache_ram(Some(0), true),
            CacheRamHealth::DisabledByUser
        );
        assert_eq!(
            classify_cache_ram(Some(0), false),
            CacheRamHealth::DisabledInsufficientRam
        );
    }

    /// Only the forced zero is a problem; a chosen one is working as asked.
    #[test]
    fn only_forced_zero_needs_attention() {
        assert!(!CacheRamHealth::DisabledByUser.needs_attention());
        assert!(CacheRamHealth::DisabledInsufficientRam.needs_attention());
        assert!(!CacheRamHealth::LlamaDefault.needs_attention());
        assert!(CacheRamHealth::Low { mb: 2048 }.needs_attention());
        assert!(!CacheRamHealth::Healthy { mb: 70_000 }.needs_attention());
    }

    #[test]
    fn classify_flags_a_cramped_budget_as_low() {
        assert_eq!(
            classify_cache_ram(Some(2048), false),
            CacheRamHealth::Low { mb: 2048 }
        );
    }

    /// The watermark is inclusive, and one MiB past it is healthy.
    #[test]
    fn low_watermark_boundary_is_inclusive() {
        let at = CACHE_RAM_LOW_WATERMARK_BYTES / (1024 * 1024);
        assert_eq!(
            classify_cache_ram(Some(at), false),
            CacheRamHealth::Low { mb: at }
        );
        assert_eq!(
            classify_cache_ram(Some(at + 1), false),
            CacheRamHealth::Healthy { mb: at + 1 }
        );
    }

    /// A small budget is equally cramped whichever way it was chosen — unlike
    /// zero, the consequence doesn't depend on intent.
    #[test]
    fn low_classification_ignores_explicitness() {
        assert_eq!(
            classify_cache_ram(Some(1024), true),
            classify_cache_ram(Some(1024), false)
        );
    }

    /// The smallest budget `compute_auto_cache_ram_mb` can emit without
    /// collapsing to zero must classify as Low, not Healthy — otherwise the
    /// warning band has a gap right where it matters most.
    #[test]
    fn smallest_nonzero_auto_budget_classifies_as_low() {
        // 24 GiB machine lands exactly on the floor: 1024 MiB.
        let mb = compute_auto_cache_ram_mb(24 * GIB, 7 * GIB, 0);
        assert_eq!(mb, 1024);
        assert_eq!(
            classify_cache_ram(Some(mb), false),
            CacheRamHealth::Low { mb }
        );
    }

    /// The unknown-KV allowance is generous enough to shrink, never inflate,
    /// the budget relative to a known small KV.
    #[test]
    fn unknown_kv_allowance_is_conservative() {
        let known_small = compute_auto_cache_ram_mb(64 * GIB, 10 * GIB, GIB);
        let unknown =
            compute_auto_cache_ram_mb(64 * GIB, 10 * GIB, CACHE_RAM_UNKNOWN_KV_ALLOWANCE_BYTES);
        assert!(
            unknown <= known_small,
            "unknown-KV budget {unknown} should not exceed known-KV {known_small}"
        );
    }
}
