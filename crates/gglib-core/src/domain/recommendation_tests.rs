//! Tests for the two shortlists and [`super::recommend_from`].

use super::*;

const GB: u64 = 1_073_741_824;

fn vram(gb: u64) -> SystemMemoryInfo {
    SystemMemoryInfo {
        total_ram_bytes: 64 * GB,
        gpu_memory_bytes: Some(gb * GB),
        is_unified_memory: false,
        has_nvidia_gpu: true,
    }
}

fn ram_only(gb: u64) -> SystemMemoryInfo {
    SystemMemoryInfo {
        total_ram_bytes: gb * GB,
        gpu_memory_bytes: None,
        is_unified_memory: false,
        has_nvidia_gpu: false,
    }
}

fn unified(bytes: u64) -> SystemMemoryInfo {
    SystemMemoryInfo {
        total_ram_bytes: bytes,
        gpu_memory_bytes: Some(bytes),
        is_unified_memory: true,
        has_nvidia_gpu: false,
    }
}

/// The tables are hand-maintained, so guard the invariants a careless edit
/// would break. This cannot check the byte counts are *correct* — only
/// Hugging Face can — but it does catch a row inserted out of order, which
/// would silently make `recommend_from` return an undersized model.
#[test]
fn shortlist_is_internally_consistent() {
    for (table, list) in [
        ("SHORTLIST", SHORTLIST),
        ("PHONE_SHORTLIST", PHONE_SHORTLIST),
    ] {
        assert!(!list.is_empty(), "{table} is empty");
        for c in list {
            assert!(
                c.required_bytes() > c.weights_bytes,
                "{table} {}: KV cache must cost something",
                c.repo
            );
            assert!(c.context > 0, "{table} {}: context must be set", c.repo);
            assert!(
                !c.rationale.is_empty(),
                "{table} {}: needs a rationale",
                c.repo
            );
        }
        for pair in list.windows(2) {
            assert!(
                pair[0].required_bytes() > pair[1].required_bytes(),
                "{table} must be ordered largest-first: {} is not bigger than {}",
                pair[0].repo,
                pair[1].repo,
            );
        }
    }
}

/// Phone-sized memory readings, as the unified memory a phone has. The
/// table tops out at the 4B, so every one of these is offered it.
#[test]
fn phone_readings_select_the_largest_phone_model() {
    for gb in [4, 6, 8, 12] {
        let got = recommend_from(PHONE_SHORTLIST, &unified(gb * GB))
            .unwrap_or_else(|| panic!("{gb} GB found nothing"));
        assert_eq!(got.candidate.repo, "unsloth/Qwen3-4B-GGUF", "at {gb} GB");
        assert_eq!(got.budget_source, BudgetSource::UnifiedMemory);
    }
}

/// Each phone row is offered from its own edge up to the next row's, and
/// nothing below the smallest, to the byte. The budgets come from the rows, so
/// a row edited without re-sorting the table fails here too.
#[test]
fn each_phone_row_is_offered_from_its_own_edge() {
    for (i, c) in PHONE_SHORTLIST.iter().enumerate() {
        let at_edge = recommend_from(PHONE_SHORTLIST, &unified(c.min_budget_bytes()));
        assert_eq!(at_edge.map(|r| r.candidate.repo), Some(c.repo));
        let below = recommend_from(PHONE_SHORTLIST, &unified(c.min_budget_bytes() - 1));
        let next = PHONE_SHORTLIST.get(i + 1).map(|n| n.repo);
        assert_eq!(below.map(|r| r.candidate.repo), next, "below {}", c.repo);
    }
}

/// What the phone table offers, the phone's verdict calls `Fits`, from the
/// row's own edge: the rows are sized at the second rung, both sides use the
/// default KV types, and both read a budget through [`usable_share`]. A byte
/// below the edge the row is no longer offered, and the verdict is `Tight`.
#[test]
fn a_phone_suggestion_is_a_fit() {
    use crate::domain::phone_fit::{FitVerdict, fit_verdict};
    for c in PHONE_SHORTLIST {
        let verdict_at = |budget| {
            fit_verdict(
                Some(c.context),
                Some(c.weights_bytes),
                Some(c.kv_elems_per_token),
                Some(budget),
            )
            .0
        };
        let edge = c.min_budget_bytes();
        assert_eq!(
            verdict_at(edge),
            FitVerdict::Fits { ctx: c.context },
            "{}",
            c.repo
        );
        assert_eq!(
            verdict_at(edge - 1),
            FitVerdict::Tight { ctx: 4096 },
            "{}",
            c.repo
        );
    }
}

/// [`budget_covering`] is the exact inverse of [`usable_share`]: its answer
/// covers the bytes and a byte less does not, across byte counts from nothing
/// to a desktop card's, and at none a budget could cover.
#[test]
fn budget_covering_is_the_least_budget_whose_usable_share_covers() {
    let small = 0..=4096;
    let phone_and_card = (0..=24 * GB).step_by(7_919_993);
    for bytes in small.chain(phone_and_card) {
        let budget = budget_covering(bytes);
        assert!(usable_share(budget) >= bytes, "{bytes}: {budget} is short");
        assert!(
            budget == 0 || usable_share(budget - 1) < bytes,
            "{bytes}: {budget} is not the least"
        );
    }
    assert_eq!(budget_covering(u64::MAX), u64::MAX);
}

/// The tiers this exists to serve. A change that shifts any of these is a
/// product decision, not a refactor.
#[test]
fn each_tier_selects_the_expected_model() {
    let cases = [
        (24, "unsloth/Qwen3-30B-A3B-GGUF"),
        (16, "bartowski/Qwen2.5-Coder-14B-Instruct-GGUF"),
        (12, "bartowski/Qwen2.5-Coder-7B-Instruct-GGUF"),
        (8, "bartowski/Qwen2.5-Coder-7B-Instruct-GGUF"),
        (4, "bartowski/Qwen2.5-Coder-3B-Instruct-GGUF"),
    ];
    for (gb, expected) in cases {
        let got = recommend(&vram(gb)).unwrap_or_else(|| panic!("{gb} GB found nothing"));
        assert_eq!(got.candidate.repo, expected, "at {gb} GB");
    }
}

#[test]
fn nothing_fits_below_the_smallest_tier() {
    assert!(recommend(&vram(2)).is_none());
}

/// VRAM is the memory the weights occupy; a large host RAM figure must not
/// talk the recommendation up past what the card can hold.
#[test]
fn vram_wins_over_system_ram_when_known() {
    let mem = SystemMemoryInfo {
        total_ram_bytes: 128 * GB,
        gpu_memory_bytes: Some(8 * GB),
        is_unified_memory: false,
        has_nvidia_gpu: true,
    };
    let got = recommend(&mem).expect("8 GB fits something");
    assert_eq!(got.budget_source, BudgetSource::Vram);
    assert_eq!(got.budget_bytes, 8 * GB);
    assert_eq!(
        got.candidate.repo,
        "bartowski/Qwen2.5-Coder-7B-Instruct-GGUF"
    );
}

#[test]
fn apple_silicon_reports_unified_memory() {
    let mem = SystemMemoryInfo {
        total_ram_bytes: 32 * GB,
        gpu_memory_bytes: Some(24 * GB),
        is_unified_memory: true,
        has_nvidia_gpu: false,
    };
    let got = recommend(&mem).expect("24 GB fits something");
    assert_eq!(got.budget_source, BudgetSource::UnifiedMemory);
}

/// A device whose memory could not be read at all — a CPU-only host, or
/// one with no readable Vulkan or sysfs answer.
#[test]
fn missing_vram_falls_back_to_system_ram() {
    let got = recommend(&ram_only(32)).expect("32 GB fits something");
    assert_eq!(got.budget_source, BudgetSource::SystemRam);
    assert_eq!(got.budget_bytes, 32 * GB);
}

/// A budget exactly equal to the requirement must be refused: the reserve
/// is what stops "fits on paper" from becoming "swaps to host memory".
#[test]
fn a_budget_equal_to_the_requirement_is_not_enough() {
    let smallest = SHORTLIST.last().expect("shortlist is non-empty");
    let mem = SystemMemoryInfo {
        total_ram_bytes: smallest.required_bytes(),
        gpu_memory_bytes: Some(smallest.required_bytes()),
        is_unified_memory: false,
        has_nvidia_gpu: true,
    };
    assert!(recommend(&mem).is_none());

    // ...but the same requirement plus the reserve is.
    let mem = SystemMemoryInfo {
        gpu_memory_bytes: Some(smallest.min_budget_bytes()),
        ..mem
    };
    assert!(recommend(&mem).is_some());
}

#[test]
fn headroom_is_the_unused_remainder() {
    let got = recommend(&vram(24)).expect("24 GB fits something");
    assert_eq!(got.headroom_bytes, 24 * GB - got.candidate.required_bytes());
}

/// The KV term must track the context, or the 14B's much larger cache
/// would be invisible to the fit check.
#[test]
fn required_bytes_includes_the_kv_cache_at_the_stated_context() {
    let c = SHORTLIST[0];
    let per_token = kv_bytes_per_token(c.kv_elems_per_token, KvCacheType::Q8_0, KvCacheType::Q8_0);
    assert_eq!(c.required_bytes(), c.weights_bytes + per_token * c.context);
}
