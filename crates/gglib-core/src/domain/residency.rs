//! Whether a second model may stay resident in VRAM alongside the first.
//!
//! One model at a time is the safe default, not the good one. An embedding
//! model is often two orders of magnitude smaller than the chat model it keeps
//! displacing: `nomic-embed-text` is ~275 MB against a 9 GB coder model, and on
//! a 16–24 GB card there is room for both several times over. Every swap
//! between them costs a full process teardown, weight reload, and prompt
//! re-prefill — paid twice per alternation, for want of a few hundred megabytes.
//!
//! This module answers the one question that decides whether that cost is paid:
//! *given what is free right now, can this candidate simply stay loaded too?*
//!
//! It is deliberately pure. The live VRAM figure is supplied by the caller
//! (`gglib_runtime::system::free_gpu_memory_bytes`), so the arithmetic is
//! testable on a machine with no GPU at all — which is every CI runner this
//! workspace has.
//!
//! ## What it does not do
//!
//! It does not decide *which* model is a good co-resident, and it consults no
//! tags. A model earns the second slot by fitting, full stop. That keeps the
//! rule honest: a 275 MB embedding model and a 900 MB title generator are the
//! same problem, and a 7B chat model is refused by the ceiling rather than by a
//! category judgement that would be wrong as often as it was right.
//!
//! It also does not model host RAM. The secondary's `--cache-ram` budget is the
//! caller's problem (see `gglib_runtime`'s residency module), because that
//! figure depends on what the *primary* already took.

use crate::cache_config::KvCacheType;
use crate::domain::kv_estimate::{
    KvElemsPerToken, estimate_kv_bytes_for_context, kv_bytes_per_token,
};

/// Fraction of free VRAM a co-resident candidate is allowed to claim.
///
/// The remainder absorbs what this estimate deliberately does not model: the
/// compute buffer llama-server allocates per batch, allocator fragmentation,
/// and whatever the desktop compositor takes while the process is starting.
/// The same 0.9 that
/// [`recommendation`](crate::domain::recommendation) sizes first-run model
/// suggestions against — one convention for "do not fill the card to the brim",
/// not two.
pub const RESIDENCY_UTILISATION: f64 = 0.9;

/// Hard ceiling on a co-resident model's footprint, regardless of free VRAM.
///
/// Free VRAM alone is not a sufficient test. A 48 GB card running a 7 B model
/// has room to co-load a second 7 B model, and doing so would be wrong: the
/// second slot exists to keep *small auxiliary* models out of the swap path,
/// not to become a general-purpose multi-model server. Two large models sharing
/// a card contend for bandwidth and compute buffers in ways this estimate does
/// not capture, and the request queue already handles that case correctly by
/// swapping.
///
/// 2 GiB comfortably covers every embedding model, reranker, and small
/// title/summary generator in common use, and excludes essentially every
/// instruct model worth chatting with.
pub(crate) const SECONDARY_MAX_BYTES: u64 = 2 * 1024 * 1024 * 1024;

/// What one resident model is expected to occupy in VRAM.
///
/// Weights and KV are tracked separately rather than pre-summed because they
/// come from different places and fail differently: weights are a measured
/// file size, KV is an estimate from GGUF metadata that may be missing
/// entirely. A caller reporting a refusal wants to say which half was the
/// problem.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SlotFootprint {
    /// Model weights on disk, summed across shards. `0` when unknown.
    pub weights_bytes: u64,
    /// KV cache at the context this launch will use.
    pub kv_bytes: u64,
}

impl SlotFootprint {
    /// Assemble a footprint from a model's launch inputs.
    ///
    /// `kv_elems_per_token` is `None` for models whose GGUF metadata does not
    /// carry the layer/head counts (see
    /// [`estimate_kv_elems_per_token`](crate::domain::estimate_kv_elems_per_token)).
    /// That yields a footprint of weights alone, which understates the true
    /// cost — so [`decide_secondary_slot`] treats an unknown KV as
    /// disqualifying rather than free. See
    /// [`SecondarySlotDecision::RefuseUnknownFootprint`].
    #[must_use]
    pub const fn new(
        weights_bytes: u64,
        kv_elems_per_token: Option<KvElemsPerToken>,
        cache_type_k: KvCacheType,
        cache_type_v: KvCacheType,
        context_size: u64,
    ) -> Option<Self> {
        // `const fn` cannot use `?` on Option in a match arm position here, so
        // this is spelled out.
        match kv_elems_per_token {
            Some(elems) => {
                let per_token = kv_bytes_per_token(elems, cache_type_k, cache_type_v);
                Some(Self {
                    weights_bytes,
                    kv_bytes: estimate_kv_bytes_for_context(per_token, context_size),
                })
            }
            None => None,
        }
    }

    /// Total VRAM this model is expected to occupy.
    #[must_use]
    pub const fn total(&self) -> u64 {
        self.weights_bytes.saturating_add(self.kv_bytes)
    }
}

/// Whether a candidate may take the second resident slot, and why not when it
/// may not.
///
/// A reason-carrying enum rather than a `bool` for the same reason
/// [`CacheRamHealth`](crate::domain::CacheRamHealth) is one: the dashboard has
/// to explain an empty second slot to a user who can see they have 12 GB free,
/// and "no" on its own is the least useful answer available.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecondarySlotDecision {
    /// The candidate fits with headroom to spare and may be co-loaded.
    Grant {
        /// What the candidate is expected to occupy.
        footprint_bytes: u64,
        /// Free VRAM left over after it, before the utilisation margin.
        headroom_bytes: u64,
    },
    /// The candidate exceeds [`SECONDARY_MAX_BYTES`]. The second slot is for
    /// auxiliary models; this one belongs in the swap path.
    RefuseTooLarge {
        /// What the candidate is expected to occupy.
        footprint_bytes: u64,
        /// The ceiling it exceeded.
        ceiling_bytes: u64,
    },
    /// Small enough in principle, but there is not enough free VRAM right now.
    RefuseNoHeadroom {
        /// What the candidate is expected to occupy.
        footprint_bytes: u64,
        /// Free VRAM at the moment of the decision.
        free_bytes: u64,
    },
    /// The candidate's KV footprint could not be estimated, so its true cost is
    /// unknown. Refused rather than guessed: an under-estimate co-loads a model
    /// that then OOMs the primary mid-generation.
    RefuseUnknownFootprint,
    /// gglib cannot read this machine's free VRAM — every non-NVIDIA,
    /// non-Apple-Silicon GPU, and every CPU-only host. Single-slot behaviour is
    /// preserved exactly.
    RefuseUnknownBudget,
}

impl SecondarySlotDecision {
    /// Whether the candidate may be co-loaded.
    #[must_use]
    pub const fn is_grant(&self) -> bool {
        matches!(self, Self::Grant { .. })
    }

    /// Stable machine-readable label, for styling and for telemetry that should
    /// not have to parse prose.
    #[must_use]
    pub const fn label(&self) -> &'static str {
        match self {
            Self::Grant { .. } => "grant",
            Self::RefuseTooLarge { .. } => "too_large",
            Self::RefuseNoHeadroom { .. } => "no_headroom",
            Self::RefuseUnknownFootprint => "unknown_footprint",
            Self::RefuseUnknownBudget => "unknown_budget",
        }
    }
}

/// Decide whether `candidate` may stay resident alongside what is already
/// loaded.
///
/// `free_vram_bytes` is the *live* figure — what is actually free on the device
/// now, with the primary model already loaded — not the card's nominal
/// capacity. `None` means gglib could not read it, which is a refusal rather
/// than an assumption in either direction.
///
/// The candidate must clear both tests: the absolute ceiling
/// ([`SECONDARY_MAX_BYTES`]) and the live budget scaled by
/// [`RESIDENCY_UTILISATION`]. The ceiling is checked first so a large model on
/// a large card reports the reason that will still be true tomorrow.
#[must_use]
pub fn decide_secondary_slot(
    candidate: Option<SlotFootprint>,
    free_vram_bytes: Option<u64>,
) -> SecondarySlotDecision {
    let Some(candidate) = candidate else {
        return SecondarySlotDecision::RefuseUnknownFootprint;
    };
    let footprint_bytes = candidate.total();

    if footprint_bytes > SECONDARY_MAX_BYTES {
        return SecondarySlotDecision::RefuseTooLarge {
            footprint_bytes,
            ceiling_bytes: SECONDARY_MAX_BYTES,
        };
    }

    let Some(free_bytes) = free_vram_bytes else {
        return SecondarySlotDecision::RefuseUnknownBudget;
    };

    #[allow(clippy::cast_precision_loss, clippy::cast_sign_loss)]
    #[allow(clippy::cast_possible_truncation)]
    let usable = (free_bytes as f64 * RESIDENCY_UTILISATION) as u64;

    if footprint_bytes > usable {
        return SecondarySlotDecision::RefuseNoHeadroom {
            footprint_bytes,
            free_bytes,
        };
    }

    SecondarySlotDecision::Grant {
        footprint_bytes,
        headroom_bytes: free_bytes.saturating_sub(footprint_bytes),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GIB: u64 = 1024 * 1024 * 1024;
    const MIB: u64 = 1024 * 1024;

    /// A `nomic-embed-text`-shaped candidate: tiny weights, tiny KV.
    fn embedder() -> SlotFootprint {
        SlotFootprint {
            weights_bytes: 275 * MIB,
            kv_bytes: 32 * MIB,
        }
    }

    #[test]
    fn a_small_model_with_room_to_spare_is_granted() {
        let decision = decide_secondary_slot(Some(embedder()), Some(8 * GIB));

        match decision {
            SecondarySlotDecision::Grant {
                footprint_bytes,
                headroom_bytes,
            } => {
                assert_eq!(footprint_bytes, 307 * MIB);
                assert_eq!(headroom_bytes, 8 * GIB - 307 * MIB);
            }
            other => panic!("expected a grant, got {other:?}"),
        }
        assert!(decision.is_grant());
        assert_eq!(decision.label(), "grant");
    }

    /// The ceiling is absolute: free VRAM cannot buy a large model into the
    /// second slot, because the second slot is not what large models are for.
    #[test]
    fn a_large_model_is_refused_even_on_a_card_with_room() {
        let big = SlotFootprint {
            weights_bytes: 9 * GIB,
            kv_bytes: GIB,
        };

        match decide_secondary_slot(Some(big), Some(40 * GIB)) {
            SecondarySlotDecision::RefuseTooLarge {
                footprint_bytes,
                ceiling_bytes,
            } => {
                assert_eq!(footprint_bytes, 10 * GIB);
                assert_eq!(ceiling_bytes, SECONDARY_MAX_BYTES);
            }
            other => panic!("expected RefuseTooLarge, got {other:?}"),
        }
    }

    /// Ordering matters: a model that is both over the ceiling *and* over
    /// budget reports the ceiling, because that reason survives the card
    /// emptying out.
    #[test]
    fn the_ceiling_is_reported_before_the_live_budget() {
        let big = SlotFootprint {
            weights_bytes: 9 * GIB,
            kv_bytes: 0,
        };

        assert!(matches!(
            decide_secondary_slot(Some(big), Some(128 * MIB)),
            SecondarySlotDecision::RefuseTooLarge { .. }
        ));
    }

    #[test]
    fn a_small_model_is_refused_when_the_card_is_nearly_full() {
        match decide_secondary_slot(Some(embedder()), Some(200 * MIB)) {
            SecondarySlotDecision::RefuseNoHeadroom {
                footprint_bytes,
                free_bytes,
            } => {
                assert_eq!(footprint_bytes, 307 * MIB);
                assert_eq!(free_bytes, 200 * MIB);
            }
            other => panic!("expected RefuseNoHeadroom, got {other:?}"),
        }
    }

    /// The utilisation margin is the point: a candidate that fits the raw free
    /// figure but not the scaled one must be refused, or it "almost fits" —
    /// the slowest possible outcome, since llama.cpp spills to host memory
    /// rather than failing.
    #[test]
    fn a_candidate_that_only_fits_without_the_margin_is_refused() {
        let footprint = SlotFootprint {
            weights_bytes: 950 * MIB,
            kv_bytes: 0,
        };
        // 950 MiB fits inside 1000 MiB free, but not inside 0.9 x 1000 = 900.
        assert!(matches!(
            decide_secondary_slot(Some(footprint), Some(1000 * MIB)),
            SecondarySlotDecision::RefuseNoHeadroom { .. }
        ));
    }

    /// The boundary itself is inclusive — exactly the usable budget is a fit.
    #[test]
    fn a_candidate_at_exactly_the_usable_budget_is_granted() {
        let free = 1000 * MIB;
        #[allow(clippy::cast_precision_loss, clippy::cast_sign_loss)]
        #[allow(clippy::cast_possible_truncation)]
        let usable = (free as f64 * RESIDENCY_UTILISATION) as u64;
        let footprint = SlotFootprint {
            weights_bytes: usable,
            kv_bytes: 0,
        };

        assert!(decide_secondary_slot(Some(footprint), Some(free)).is_grant());
    }

    /// Every Vulkan-only, AMD, Intel, and CPU-only host lands here. Refusing
    /// keeps single-slot behaviour byte-for-byte identical to before M9.
    #[test]
    fn an_unreadable_vram_budget_refuses_rather_than_assuming() {
        assert_eq!(
            decide_secondary_slot(Some(embedder()), None),
            SecondarySlotDecision::RefuseUnknownBudget
        );
    }

    /// A model whose KV shape could not be estimated has an unknown true cost.
    /// Treating the missing half as zero would co-load it against a budget it
    /// does not actually fit.
    #[test]
    fn an_unknown_footprint_refuses_rather_than_undercounting() {
        assert_eq!(
            decide_secondary_slot(None, Some(64 * GIB)),
            SecondarySlotDecision::RefuseUnknownFootprint
        );
    }

    // ── SlotFootprint::new ────────────────────────────────────────────────

    #[test]
    fn footprint_sums_weights_and_kv_at_the_launch_context() {
        let elems = KvElemsPerToken { k: 1024, v: 1024 };
        let footprint = SlotFootprint::new(
            500 * MIB,
            Some(elems),
            KvCacheType::Q8_0,
            KvCacheType::Q8_0,
            8192,
        )
        .expect("known KV shape yields a footprint");

        let per_token = kv_bytes_per_token(elems, KvCacheType::Q8_0, KvCacheType::Q8_0);
        assert_eq!(footprint.weights_bytes, 500 * MIB);
        assert_eq!(
            footprint.kv_bytes,
            estimate_kv_bytes_for_context(per_token, 8192)
        );
        assert_eq!(footprint.total(), 500 * MIB + footprint.kv_bytes);
    }

    #[test]
    fn footprint_is_unknown_when_the_kv_shape_is() {
        assert_eq!(
            SlotFootprint::new(500 * MIB, None, KvCacheType::Q8_0, KvCacheType::Q8_0, 8192),
            None
        );
    }

    /// Quantized KV is what the runtime actually launches with, so a footprint
    /// computed against `f16` would over-reserve by roughly the KV cache again
    /// and refuse co-loads that would have fitted.
    #[test]
    fn kv_cache_type_changes_the_footprint() {
        let elems = KvElemsPerToken { k: 4096, v: 4096 };
        let quantized =
            SlotFootprint::new(0, Some(elems), KvCacheType::Q8_0, KvCacheType::Q8_0, 32_768)
                .unwrap();
        let full =
            SlotFootprint::new(0, Some(elems), KvCacheType::F16, KvCacheType::F16, 32_768).unwrap();

        assert!(
            quantized.total() < full.total(),
            "q8_0 {} should be smaller than f16 {}",
            quantized.total(),
            full.total()
        );
    }
}
