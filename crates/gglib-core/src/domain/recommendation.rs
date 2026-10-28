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
//! A phone has a table of its own, [`PHONE_SHORTLIST`]: small models chosen
//! for plain chat, sized against a phone's memory rather than a desktop
//! card's. [`recommend_from`] is the one way of choosing from either table;
//! [`recommend`] is it over `gglib up`'s.
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

/// The share of `budget_bytes` a model may occupy: [`BUDGET_UTILISATION`] of
/// it, rounded down. The context fit spends exactly this much of a budget.
#[must_use]
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::cast_possible_truncation,
    reason = "a byte count scaled by a fraction under one, rounded down"
)]
pub fn usable_share(budget_bytes: u64) -> u64 {
    (budget_bytes as f64 * BUDGET_UTILISATION) as u64
}

/// The smallest budget whose [`usable_share`] is at least `bytes`: what a
/// model occupying `bytes` has to be given. `u64::MAX` when no budget is.
///
/// Searched for from the quotient rather than taken as it, because
/// [`BUDGET_UTILISATION`] is not exact in binary and `bytes` divided by it can
/// land a byte either side of the edge [`usable_share`] rounds to.
#[must_use]
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::cast_possible_truncation,
    reason = "the quotient only seeds the search; the search makes it exact"
)]
pub fn budget_covering(bytes: u64) -> u64 {
    if usable_share(u64::MAX) < bytes {
        return u64::MAX;
    }
    let mut budget = (bytes as f64 / BUDGET_UTILISATION).ceil() as u64;
    while usable_share(budget) < bytes {
        budget += 1;
    }
    while budget > 0 && usable_share(budget - 1) >= bytes {
        budget -= 1;
    }
    budget
}

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

    /// The smallest memory budget this candidate may be recommended for: the
    /// least whose usable share covers [`required_bytes`](Self::required_bytes),
    /// from [`budget_covering`].
    #[must_use]
    pub fn min_budget_bytes(&self) -> u64 {
        budget_covering(self.required_bytes())
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
    /// Unified memory — Apple Silicon, or an integrated GPU.
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
/// against the repositories when this table is edited. The same holds for
/// [`PHONE_SHORTLIST`].
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

/// The phone shortlist, largest first.
///
/// Small models for plain chat on a phone, so the module's tool-call bias does
/// not decide this table: Qwen3 writes tool calls in the `<tool_call>`
/// envelope the normaliser parses, Llama 3.2 in Llama 3's bare JSON, for which
/// the template probe derives no dialect, and Gemma 3's template has no tool
/// calls at all. Every repository is public, so none needs a Hugging Face
/// token.
///
/// Each row is sized at 8192 tokens, the second rung of `context_fit`'s
/// ladder, so what this table offers a phone is a model
/// [`fit_verdict`](crate::domain::phone_fit::fit_verdict) calls `Fits` at the
/// same budget, not `Tight`, at a row's edge too: both read a budget through
/// [`usable_share`].
///
/// The rows are provisional: gglib has no reading of a phone running them, so
/// neither the memory a phone can give them nor the 8192-token column rests on
/// a measurement. Weights are the exact size of each file on Hugging Face at
/// the revision its row names; KV shapes are read from each file's own GGUF
/// metadata and counted as
/// [`estimate_kv_elems_per_token`](crate::domain::estimate_kv_elems_per_token)
/// counts them.
pub static PHONE_SHORTLIST: &[ModelCandidate] = &[
    ModelCandidate {
        repo: "unsloth/Qwen3-4B-GGUF",
        quantization: "Q4_K_M",
        // Revision 22c9fc8a8c7700b76a1789366280a6a5a1ad1120.
        weights_bytes: 2_497_281_312,
        // 36 layers x 8 KV heads x 128 head dim; trained to 40960.
        kv_elems_per_token: KvElemsPerToken {
            k: 36_864,
            v: 36_864,
        },
        context: 8192,
        rationale: "the strongest model here a phone can hold, and it thinks \
                    before it answers",
    },
    ModelCandidate {
        repo: "bartowski/Llama-3.2-3B-Instruct-GGUF",
        quantization: "Q4_K_M",
        // Revision 5ab33fa94d1d04e903623ae72c95d1696f09f9e8.
        weights_bytes: 2_019_377_696,
        // 28 layers x 8 KV heads x 128 head dim; trained to 131072.
        kv_elems_per_token: KvElemsPerToken {
            k: 28_672,
            v: 28_672,
        },
        context: 8192,
        rationale: "Meta's small chat model: it answers straight away, with no \
                    thinking to wait through",
    },
    ModelCandidate {
        repo: "unsloth/Qwen3-1.7B-GGUF",
        quantization: "Q4_K_M",
        // Revision d7f544eead698dbd1f15126ef60b45a1e1933222.
        weights_bytes: 1_107_409_472,
        // 28 layers x 8 KV heads x 128 head dim; trained to 40960.
        kv_elems_per_token: KvElemsPerToken {
            k: 28_672,
            v: 28_672,
        },
        context: 8192,
        rationale: "Qwen3's thinking in about half the memory of the 4B",
    },
    ModelCandidate {
        repo: "bartowski/Llama-3.2-1B-Instruct-GGUF",
        quantization: "Q4_K_M",
        // Revision 067b946cf014b7c697f3654f621d577a3e3afd1c.
        weights_bytes: 807_694_464,
        // 16 layers x 8 KV heads x 64 head dim; trained to 131072.
        kv_elems_per_token: KvElemsPerToken { k: 8192, v: 8192 },
        context: 8192,
        rationale: "Meta's smallest Llama 3.2: quick, direct answers in under a \
                    gigabyte",
    },
    ModelCandidate {
        repo: "unsloth/gemma-3-1b-it-GGUF",
        quantization: "Q4_K_M",
        // Revision f0b45be0aac41bd6a100a4b5734cad5f67255bfb.
        weights_bytes: 806_058_272,
        // 26 layers x 1 KV head x 256 head dim; trained to 32768. The file
        // says `sliding_window = 512` and has no `full_attention_interval`, so
        // every layer is counted at the full context: more than the sliding
        // layers keep, which is the safe direction.
        kv_elems_per_token: KvElemsPerToken { k: 6656, v: 6656 },
        context: 8192,
        rationale: "Google's smallest Gemma 3: short, plain answers in under a \
                    gigabyte",
    },
    ModelCandidate {
        repo: "unsloth/Qwen3-0.6B-GGUF",
        quantization: "Q4_K_M",
        // Revision 50968a4468ef4233ed78cd7c3de230dd1d61a56b.
        weights_bytes: 396_705_472,
        // 28 layers x 8 KV heads x 128 head dim; trained to 40960.
        kv_elems_per_token: KvElemsPerToken {
            k: 28_672,
            v: 28_672,
        },
        context: 8192,
        rationale: "the smallest model here, and it still thinks before it \
                    answers",
    },
];

/// Resolve the memory figure to size against, and say where it came from.
///
/// VRAM wins when it is known, because that is the memory the weights will
/// actually occupy. It is `None` on a CPU-only host, and on any device whose
/// memory gglib could not read — which falls back to host RAM. That fallback
/// is usually *too generous*, which is exactly why [`BudgetSource`] is
/// returned alongside the number instead of being thrown away: the caller is
/// expected to say so. A unified-memory device is labelled, not guessed.
const fn resolve_budget(mem: &SystemMemoryInfo) -> (u64, BudgetSource) {
    match mem.gpu_memory_bytes {
        Some(vram) if mem.is_unified_memory => (vram, BudgetSource::UnifiedMemory),
        Some(vram) => (vram, BudgetSource::Vram),
        None => (mem.total_ram_bytes, BudgetSource::SystemRam),
    }
}

/// Recommend the largest model on `gglib up`'s shortlist that fits this
/// machine; [`recommend_from`] says how.
#[must_use]
pub fn recommend(mem: &SystemMemoryInfo) -> Option<Recommendation> {
    recommend_from(SHORTLIST, mem)
}

/// Recommend the largest model in `list` that fits this machine.
///
/// `list` is ordered largest-first, as both tables are, so the first fit is
/// the best fit.
///
/// Returns `None` when even the smallest candidate would not fit. That is a
/// real answer, not a failure: suggesting a model that overflows would produce
/// a working-but-unusably-slow endpoint, and the user is better served by
/// being told their budget and left to choose.
#[must_use]
pub fn recommend_from(
    list: &'static [ModelCandidate],
    mem: &SystemMemoryInfo,
) -> Option<Recommendation> {
    let (budget_bytes, budget_source) = resolve_budget(mem);

    let candidate = list.iter().find(|c| c.min_budget_bytes() <= budget_bytes)?;

    Some(Recommendation {
        candidate,
        budget_bytes,
        budget_source,
        headroom_bytes: budget_bytes.saturating_sub(candidate.required_bytes()),
    })
}

#[cfg(test)]
#[path = "recommendation_tests.rs"]
mod tests;
