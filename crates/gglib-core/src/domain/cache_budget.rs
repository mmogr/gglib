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
