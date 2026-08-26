//! Picking a first model that actually fits the machine it will run on.
//!
//! `gglib up` has to answer a question a new user cannot: *which* GGUF should
//! land on this box. Getting it wrong is worse than not answering — a model
//! that overflows VRAM does not fail, it swaps to host memory and runs at a
//! tenth of the speed, which reads as "gglib is slow" rather than "that model
//! was too big".
//!
//! The answer is a small hand-maintained shortlist rather than a live search.
//! Hugging Face has tens of thousands of GGUF repos and no reliable signal for
//! "this one tool-calls properly"; a curated table is deterministic, testable
//! offline, and needs no network round-trip before the confirmation prompt.
//! Its cost — someone has to revisit it as models age — is paid once per
//! release rather than once per user.
//!
//! Candidates are biased towards models whose tool-call dialect
//! [`crate::normalize`] already parses. Recommending a model gglib cannot
//! normalize would sell the user the exact failure the proxy exists to fix.
//!
//! This module decides *what to suggest*; it does not download, and it has no
//! opinion on what to do when nothing fits — [`recommend`] returns [`None`]
//! and the caller reports the hardware it found.

use crate::cache_config::KvCacheType;
use crate::domain::kv_estimate::{
    KvElemsPerToken, estimate_kv_bytes_for_context, kv_bytes_per_token,
};
use crate::utils::system::SystemMemoryInfo;

/// Fraction of the memory budget a candidate is allowed to occupy.
///
/// The remainder absorbs what this estimate deliberately does not model: the
/// compute buffer, the framebuffer already in use by the desktop, allocator
/// fragmentation. Sizing to 100% of nominal VRAM reliably produces a model
/// that *almost* fits, which is the slowest possible outcome.
pub const BUDGET_UTILISATION: f64 = 0.9;

/// One entry in the shortlist.
///
/// Weights are recorded as the actual byte size of the quantized file on
/// Hugging Face — not a rounded "about 18 GB" — because the whole point is to
/// compare against a real memory figure. The KV shape comes from the model's
/// own config so the cache cost is computed by [`kv_estimate`], not restated
/// here as a second magic number.
///
/// [`kv_estimate`]: crate::domain::kv_estimate
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModelCandidate {
    /// Hugging Face repository id, passed verbatim to the download queue.
    pub repo: &'static str,
    /// Quantization to request from that repository.
    pub quantization: &'static str,
    /// Size of the quantized weights, in bytes.
    pub weights_bytes: u64,
    /// Per-token KV cache element counts, derived from the model's config.
    pub kv_elems_per_token: KvElemsPerToken,
    /// Context this candidate is sized for.
    pub context: u64,
    /// Why this model, in the user's terms. Printed verbatim.
    pub rationale: &'static str,
}

impl ModelCandidate {
    /// Total memory this candidate needs: weights plus KV cache at
    /// [`context`](Self::context), quantized to the gglib default.
    ///
    /// Uses [`kv_bytes_per_token`] with [`KvCacheType::Q8_0`] on both sides
    /// because that is what the runtime actually launches with (see
    /// `gglib_runtime::llama::args::kv_cache_type`). A recommendation computed
    /// against `f16` would over-reserve by roughly the KV cache again.
    #[must_use]
    pub const fn required_bytes(&self) -> u64 {
        let per_token = kv_bytes_per_token(
            self.kv_elems_per_token,
            KvCacheType::Q8_0,
            KvCacheType::Q8_0,
        );
        self.weights_bytes
            .saturating_add(estimate_kv_bytes_for_context(per_token, self.context))
    }

    /// The smallest memory budget this candidate may be recommended for.
    #[must_use]
    pub fn min_budget_bytes(&self) -> u64 {
        #[allow(clippy::cast_precision_loss, clippy::cast_sign_loss)]
        #[allow(clippy::cast_possible_truncation)]
        {
            (self.required_bytes() as f64 / BUDGET_UTILISATION) as u64
        }
    }
}

/// Which pool of memory the recommendation was sized against.
///
/// Worth carrying rather than inferring at the print site: "24.0 GiB VRAM" and
/// "24.0 GiB system RAM" lead to very different expectations, and the
/// [`SystemRam`](Self::SystemRam) case is frequently a *fallback* rather than a
/// CPU-only machine — see [`recommend`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BudgetSource {
    /// Discrete GPU VRAM.
    Vram,
    /// Apple Silicon unified memory.
    UnifiedMemory,
    /// Host RAM — either a CPU-only machine, or a GPU whose VRAM gglib cannot
    /// read.
    SystemRam,
}

impl BudgetSource {
    /// Short label for terminal output.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Vram => "VRAM",
            Self::UnifiedMemory => "unified memory",
            Self::SystemRam => "system RAM",
        }
    }
}

/// A candidate plus the reasoning that selected it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Recommendation {
    /// The chosen model.
    pub candidate: &'static ModelCandidate,
    /// The memory figure it was sized against.
    pub budget_bytes: u64,
    /// Where that figure came from.
    pub budget_source: BudgetSource,
    /// Budget left over after the candidate's requirement.
    pub headroom_bytes: u64,
}

/// The shortlist, largest first.
///
/// Byte sizes are the `Q4_K_M` files on Hugging Face as published; KV shapes
/// are `num_hidden_layers × num_key_value_heads × head_dim` from each model's
/// `config.json`. Both are verified by `tests::shortlist_is_internally_consistent`
/// only for self-consistency — the figures themselves have to be re-checked
/// against the repositories when this table is edited.
static SHORTLIST: &[ModelCandidate] = &[
    ModelCandidate {
        repo: "unsloth/Qwen3-30B-A3B-GGUF",
        quantization: "Q4_K_M",
        weights_bytes: 18_556_686_912,
        // 48 layers x 4 KV heads x 128 head dim.
        kv_elems_per_token: KvElemsPerToken {
            k: 24_576,
            v: 24_576,
        },
        context: 32_768,
        rationale: "mixture-of-experts: 30B of knowledge, ~3B active per token, \
                    so it answers at roughly 3B speed",
    },
    ModelCandidate {
        repo: "bartowski/Qwen2.5-Coder-14B-Instruct-GGUF",
        quantization: "Q4_K_M",
        weights_bytes: 8_988_111_072,
        // 48 layers x 8 KV heads x 128 head dim.
        kv_elems_per_token: KvElemsPerToken {
            k: 49_152,
            v: 49_152,
        },
        context: 32_768,
        rationale: "the strongest dense coding model that still leaves room for \
                    a 32k context",
    },
    ModelCandidate {
        repo: "bartowski/Qwen2.5-Coder-7B-Instruct-GGUF",
        quantization: "Q4_K_M",
        weights_bytes: 4_683_074_336,
        // 28 layers x 4 KV heads x 128 head dim.
        kv_elems_per_token: KvElemsPerToken {
            k: 14_336,
            v: 14_336,
        },
        context: 32_768,
        rationale: "dependable tool calling on a mid-range card, with headroom \
                    to spare",
    },
    ModelCandidate {
        repo: "bartowski/Qwen2.5-Coder-3B-Instruct-GGUF",
        quantization: "Q4_K_M",
        weights_bytes: 1_929_903_360,
        // 36 layers x 2 KV heads x 128 head dim.
        kv_elems_per_token: KvElemsPerToken { k: 9_216, v: 9_216 },
        context: 32_768,
        rationale: "the smallest model in the list that still calls tools \
                    reliably enough to drive an agent",
    },
];

/// Resolve the memory figure to size against, and say where it came from.
///
/// VRAM wins when it is known, because that is the memory the weights will
/// actually occupy. It is `None` on every Vulkan-only machine — gglib reads
/// VRAM for Metal and NVIDIA only — so an AMD or Intel GPU falls back to host
/// RAM. That fallback is usually *too generous*, which is exactly why
/// [`BudgetSource`] is returned alongside the number instead of being thrown
/// away: the caller is expected to say so.
const fn resolve_budget(mem: &SystemMemoryInfo) -> (u64, BudgetSource) {
    match mem.gpu_memory_bytes {
        Some(vram) if mem.is_apple_silicon => (vram, BudgetSource::UnifiedMemory),
        Some(vram) => (vram, BudgetSource::Vram),
        None => (mem.total_ram_bytes, BudgetSource::SystemRam),
    }
}

/// Recommend the largest shortlisted model that fits this machine.
///
/// Returns `None` when even the smallest candidate would not fit. That is a
/// real answer, not a failure: suggesting a model that overflows would produce
/// a working-but-unusably-slow endpoint, and the user is better served by
/// being told their budget and left to choose.
#[must_use]
pub fn recommend(mem: &SystemMemoryInfo) -> Option<Recommendation> {
    let (budget_bytes, budget_source) = resolve_budget(mem);

    // Largest-first, so the first fit is the best fit.
    let candidate = SHORTLIST
        .iter()
        .find(|c| c.min_budget_bytes() <= budget_bytes)?;

    Some(Recommendation {
        candidate,
        budget_bytes,
        budget_source,
        headroom_bytes: budget_bytes.saturating_sub(candidate.required_bytes()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const GB: u64 = 1_073_741_824;

    fn vram(gb: u64) -> SystemMemoryInfo {
        SystemMemoryInfo {
            total_ram_bytes: 64 * GB,
            gpu_memory_bytes: Some(gb * GB),
            is_apple_silicon: false,
            has_nvidia_gpu: true,
        }
    }

    fn ram_only(gb: u64) -> SystemMemoryInfo {
        SystemMemoryInfo {
            total_ram_bytes: gb * GB,
            gpu_memory_bytes: None,
            is_apple_silicon: false,
            has_nvidia_gpu: false,
        }
    }

    /// The table is hand-maintained, so guard the invariants a careless edit
    /// would break. This cannot check the byte counts are *correct* — only
    /// Hugging Face can — but it does catch a row inserted out of order, which
    /// would silently make `recommend` return an undersized model.
    #[test]
    fn shortlist_is_internally_consistent() {
        assert!(!SHORTLIST.is_empty());
        for c in SHORTLIST {
            assert!(
                c.required_bytes() > c.weights_bytes,
                "{}: KV cache must cost something",
                c.repo
            );
            assert!(c.context > 0, "{}: context must be set", c.repo);
            assert!(!c.rationale.is_empty(), "{}: needs a rationale", c.repo);
        }
        for pair in SHORTLIST.windows(2) {
            assert!(
                pair[0].required_bytes() > pair[1].required_bytes(),
                "shortlist must be ordered largest-first: {} is not bigger than {}",
                pair[0].repo,
                pair[1].repo,
            );
        }
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
            is_apple_silicon: false,
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
            is_apple_silicon: true,
            has_nvidia_gpu: false,
        };
        let got = recommend(&mem).expect("24 GB fits something");
        assert_eq!(got.budget_source, BudgetSource::UnifiedMemory);
    }

    /// Vulkan-only machines report no VRAM at all, so this is the AMD/Intel
    /// path, not just the CPU-only one.
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
            is_apple_silicon: false,
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
        let per_token =
            kv_bytes_per_token(c.kv_elems_per_token, KvCacheType::Q8_0, KvCacheType::Q8_0);
        assert_eq!(c.required_bytes(), c.weights_bytes + per_token * c.context);
    }
}
