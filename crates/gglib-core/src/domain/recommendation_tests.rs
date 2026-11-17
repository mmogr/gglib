//! Tests for the two shortlists and [`super::recommend_from`].

use super::*;
use crate::domain::context_fit::fit_context_explained;

const GB: u64 = 1_073_741_824;

fn vram(gb: u64) -> SizedAgainst {
    SizedAgainst {
        budget_bytes: gb * GB,
        source: BudgetSource::Vram,
        system_ram_bytes: None,
    }
}

/// A phone's memory: the budget its load fits against, which keeps no
/// second slot ([`fit_context_explained`]).
fn phone(bytes: u64) -> Option<Recommendation> {
    recommend_from(
        PHONE_SHORTLIST,
        SizedAgainst {
            budget_bytes: bytes,
            source: BudgetSource::UnifiedMemory,
            system_ram_bytes: None,
        },
        Launching::DEFAULT,
        fit_context_explained,
    )
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
                c.context <= c.trained_ctx,
                "{table} {}: sized past the context it was trained for",
                c.repo
            );
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

/// Every row names the quantization its weights are the size of, so the
/// size a suggestion speaks of is the size its download takes.
#[test]
fn every_row_names_the_quantization_it_is_sized_at() {
    for c in SHORTLIST.iter().chain(PHONE_SHORTLIST) {
        assert_eq!(c.quantization, Some("Q4_K_M"), "{}", c.repo);
    }
}

/// A suggestion never says the machine has a card: no row's words name
/// one, and the sentence about the machine is the load's.
#[test]
fn no_row_names_a_card() {
    for c in SHORTLIST.iter().chain(PHONE_SHORTLIST) {
        for word in ["card", "GPU", "VRAM"] {
            assert!(!c.rationale.contains(word), "{}: {}", c.repo, c.rationale);
        }
    }
}

/// Phone-sized memory readings, as the unified memory a phone has. The
/// table tops out at the 4B, so every one of these is offered it.
#[test]
fn phone_readings_select_the_largest_phone_model() {
    for gb in [4, 6, 8, 12] {
        let got = phone(gb * GB).unwrap_or_else(|| panic!("{gb} GB found nothing"));
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
        let at_edge = phone(c.min_budget_bytes());
        assert_eq!(at_edge.map(|r| r.candidate.repo), Some(c.repo));
        let below = phone(c.min_budget_bytes() - 1);
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
                Some(KvShape::uniform(c.kv_elems_per_token)),
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
        let got = recommend(vram(gb), Launching::DEFAULT)
            .unwrap_or_else(|| panic!("{gb} GB found nothing"));
        assert_eq!(got.candidate.repo, expected, "at {gb} GB");
    }
}

#[test]
fn nothing_fits_below_the_smallest_tier() {
    assert!(recommend(vram(2), Launching::DEFAULT).is_none());
}

/// A row the load would serve below the context it is sized for is passed
/// over, though a least budget for it is exceeded: on a 7 GiB card the 7B
/// fits the card less a second slot's room only at 8K, and a load serves it
/// there, so the 3B, which the load serves at 32K, is suggested.
#[test]
fn a_row_the_load_serves_below_its_context_is_passed_over() {
    let seven_b = SHORTLIST
        .iter()
        .find(|c| c.repo == "bartowski/Qwen2.5-Coder-7B-Instruct-GGUF")
        .expect("the 7B is on the list");
    assert!(seven_b.min_budget_bytes() < 7 * GB);
    let got = recommend(vram(7), Launching::DEFAULT).expect("7 GB fits something");
    assert_eq!(
        got.candidate.repo,
        "bartowski/Qwen2.5-Coder-3B-Instruct-GGUF"
    );
}

/// A budget exactly equal to the requirement does not serve the row its
/// context: the reserve is what stops "fits on paper" from becoming "swaps
/// to host memory". It is suggested there only as the fallback, at the less
/// context the load serves, and says so.
#[test]
fn a_budget_equal_to_the_requirement_is_not_enough_for_its_context() {
    let smallest = SHORTLIST.last().expect("shortlist is non-empty");
    let at = |budget_bytes| SizedAgainst {
        budget_bytes,
        ..vram(0)
    };
    let short = recommend(at(smallest.required_bytes()), Launching::DEFAULT)
        .expect("the fallback suggests the 3B");
    assert!(short.fit.serves_at < smallest.context);

    // ...but the same requirement plus the reserve is.
    let full = recommend(at(smallest.min_budget_bytes()), Launching::DEFAULT).expect("suggested");
    assert_eq!(full.fit.serves_at, smallest.context);
}

/// When no row reaches its context, the row served the most context is
/// suggested, provided it is at least 8K, and its sentence says what it
/// gets; a model served only 4K is not suggested.
#[test]
fn the_fallback_suggests_the_most_context_from_8k() {
    // A 4.5 GiB card: the 3B fits it less a second slot's room only at 16K,
    // and the 7B fits nowhere.
    let got = recommend(
        SizedAgainst {
            budget_bytes: 45 * GB / 10,
            ..vram(0)
        },
        Launching::DEFAULT,
    )
    .expect("the fallback suggests something");
    assert_eq!(
        got.candidate.repo,
        "bartowski/Qwen2.5-Coder-3B-Instruct-GGUF"
    );
    assert_eq!(got.fit.serves_at, 16_384);
    assert_eq!(
        got.fit.sentence,
        "Fits this machine with 16K context, of 32K trained."
    );
}

/// A `default-context-size` outranks the fit, as it does a launch: the
/// sentence says what is served and what fits.
#[test]
fn a_default_context_size_is_what_the_suggestion_says_is_served() {
    let got = recommend(
        vram(8),
        Launching {
            default_ctx: Some(65_536),
            ..Launching::DEFAULT
        },
    )
    .expect("8 GB fits something");
    assert_eq!(got.fit.serves_at, 65_536);
    assert_eq!(
        got.fit.sentence,
        "Serves at 64K context, more than the 32K it was trained for; 32K fits this machine."
    );
}

/// The KV cache types a launch uses are the ones a row is sized at: at
/// `f16` the 7B's cache costs about twice as much, and an 8 GiB card no
/// longer serves it 32K.
#[test]
fn the_launchs_kv_types_size_the_suggestion() {
    let f16 = Launching {
        kv_types: (KvCacheType::F16, KvCacheType::F16),
        ..Launching::DEFAULT
    };
    let q8 = recommend(vram(8), Launching::DEFAULT).expect("suggested");
    let at_f16 = recommend(vram(8), f16).expect("suggested");
    assert_eq!(
        q8.candidate.repo,
        "bartowski/Qwen2.5-Coder-7B-Instruct-GGUF"
    );
    assert_ne!(
        (at_f16.candidate.repo, at_f16.fit.serves_at),
        (q8.candidate.repo, q8.fit.serves_at)
    );
}

#[test]
fn headroom_is_the_unused_remainder() {
    let got = recommend(vram(24), Launching::DEFAULT).expect("24 GB fits something");
    assert_eq!(got.headroom_bytes, 24 * GB - got.candidate.required_bytes());
}

/// The suggestion says what the load will say once the model is down: the
/// load's own sentence, naming the RAM on a machine with no GPU and nothing
/// of a card.
#[test]
fn a_suggestion_says_the_loads_sentence() {
    let card = recommend(vram(8), Launching::DEFAULT).expect("8 GB fits something");
    assert_eq!(
        card.fit.sentence,
        "Fits this machine with 32K context, of 32K trained."
    );

    let ram = 79 * GB / 10;
    let no_gpu = recommend(
        SizedAgainst {
            budget_bytes: ram / 4 * 3,
            source: BudgetSource::SystemRam,
            system_ram_bytes: Some(ram),
        },
        Launching::DEFAULT,
    )
    .expect("7.9 GiB of RAM fits something");
    assert_eq!(
        no_gpu.fit.sentence,
        "Fits this machine's 7.9 GiB of RAM (no GPU) with 32K context, of 32K trained."
    );
}

/// The KV term must track the context, or the 14B's much larger cache
/// would be invisible to the fit check.
#[test]
fn required_bytes_includes_the_kv_cache_at_the_stated_context() {
    let c = SHORTLIST[0];
    let per_token = KvShape::uniform(c.kv_elems_per_token)
        .bytes(KvCacheType::Q8_0, KvCacheType::Q8_0)
        .per_token;
    assert_eq!(c.required_bytes(), c.weights_bytes + per_token * c.context);
}
