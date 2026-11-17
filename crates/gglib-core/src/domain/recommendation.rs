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
//! Candidates are biased towards models that call tools well.
//!
//! A phone has a table of its own, [`PHONE_SHORTLIST`]: small models chosen
//! for plain chat, sized against a phone's memory rather than a desktop
//! card's. [`recommend_from`] is the one way of choosing from either table,
//! each by the rule its own load fits a context by; [`recommend`] is it over
//! `gglib up`'s, by the desktop load's rule
//! ([`fit_on_device`]), so a model
//! is suggested only when the load would serve it the context its row is
//! sized for, and the suggestion says it in the load's own sentence.
//!
//! This module decides *what to suggest*; it does not download, and it has no
//! opinion on what to do when nothing fits — [`recommend`] returns [`None`]
//! and the caller reports the hardware it found.

use crate::cache_config::{DEFAULT_CACHE_TYPE_K, DEFAULT_CACHE_TYPE_V, KvCacheType};
use crate::domain::context_fit::{FitInputs, SECOND_RUNG};
use crate::domain::device_fit::fit_on_device;
use crate::domain::kv_estimate::{KvElemsPerToken, KvShape};
use crate::domain::launch_plan::ContextFit;
use crate::server_config::ContextSizeSource;

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
    /// The quantization a download of the row asks for: the one
    /// [`weights_bytes`](Self::weights_bytes) is the size of, so the size a
    /// suggestion speaks of is the size downloaded. `None` asks for none,
    /// and the download takes the largest quantization that fits the
    /// machine, which the row's figures say nothing of.
    pub quantization: Option<&'static str>,
    /// Size of the [`quantization`](Self::quantization)'s weights, in
    /// bytes: the size the row is checked at.
    pub weights_bytes: u64,
    /// Per-token KV cache element counts, derived from the model's config.
    pub kv_elems_per_token: KvElemsPerToken,
    /// Context this candidate is sized for.
    pub context: u64,
    /// The context the model was trained for, its GGUF's
    /// `{arch}.context_length`: no fit of this row serves more.
    pub trained_ctx: u64,
    /// Why this model, in the user's terms. Printed verbatim.
    pub rationale: &'static str,
}

impl ModelCandidate {
    /// Total memory this candidate needs: weights plus KV cache at
    /// [`context`](Self::context), at the KV cache types a load uses unless
    /// told otherwise ([`DEFAULT_CACHE_TYPE_K`], [`DEFAULT_CACHE_TYPE_V`]),
    /// the ones the fit sizes a context at.
    #[must_use]
    pub const fn required_bytes(&self) -> u64 {
        self.weights_bytes.saturating_add(
            self.kv()
                .bytes(DEFAULT_CACHE_TYPE_K, DEFAULT_CACHE_TYPE_V)
                .at(self.context),
        )
    }

    /// The row's KV cache shape.
    const fn kv(&self) -> KvShape {
        KvShape::uniform(self.kv_elems_per_token)
    }

    /// The context a load fits this row to inside `budget_bytes` by `rule`,
    /// at `launching`'s KV cache types, and what it worked from.
    fn fit_by(
        &self,
        rule: FitRule,
        launching: Launching,
        budget_bytes: u64,
    ) -> (Option<u64>, FitInputs) {
        let (k, v) = launching.kv_types;
        rule(
            Some(self.trained_ctx),
            Some(self.weights_bytes),
            Some(self.kv()),
            k,
            v,
            Some(budget_bytes),
        )
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
/// "24.0 GiB system RAM" lead to very different expectations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BudgetSource {
    /// Discrete GPU VRAM.
    Vram,
    /// Unified memory — Apple Silicon, or an integrated GPU.
    UnifiedMemory,
    /// Host RAM, on a machine with no GPU: the model runs from it.
    SystemRam,
}

/// The memory a load on this machine sizes a context against, as the
/// suggestion is told it: the one figure the load reads
/// (`gglib_runtime::system::fit_memory`), never a second reading.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SizedAgainst {
    /// The budget the load fits against: a GPU's memory, or with no GPU the
    /// share of RAM it spends.
    pub budget_bytes: u64,
    /// Where that budget comes from.
    pub source: BudgetSource,
    /// The machine's whole RAM when the budget is a share of it (no GPU),
    /// which the load's sentence names; `None` for a GPU's memory.
    pub system_ram_bytes: Option<u64>,
}

impl SizedAgainst {
    /// The budget in words: `8.0 GiB of VRAM`, or with no GPU the share and
    /// the RAM it is of, `4.5 GiB of this machine's 6.0 GiB of RAM (no GPU)`.
    #[must_use]
    pub fn words(&self) -> String {
        let budget = crate::domain::format_gib(self.budget_bytes);
        self.system_ram_bytes.map_or_else(
            || format!("{budget} of {}", self.source.label()),
            |ram| {
                format!(
                    "{budget} of this machine's {} of RAM (no GPU)",
                    crate::domain::format_gib(ram)
                )
            },
        )
    }
}

/// How a load fits a context inside a budget: the desktop's
/// ([`fit_on_device`]), or a phone's, which keeps no second slot
/// ([`fit_context_explained`](crate::domain::fit_context_explained)).
pub type FitRule = fn(
    Option<u64>,
    Option<u64>,
    Option<KvShape>,
    KvCacheType,
    KvCacheType,
    Option<u64>,
) -> (Option<u64>, FitInputs);

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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recommendation {
    /// The chosen model.
    pub candidate: &'static ModelCandidate,
    /// The memory figure it was sized against.
    pub budget_bytes: u64,
    /// Where that figure came from.
    pub budget_source: BudgetSource,
    /// Budget left over after the candidate's requirement.
    pub headroom_bytes: u64,
    /// The load's fit of the candidate's
    /// [`quantization`](ModelCandidate::quantization) on this machine, as a
    /// load of it would say it: its `sentence` is the inspector's once the
    /// model is downloaded.
    pub fit: ContextFit,
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
        quantization: Some("Q4_K_M"),
        weights_bytes: 18_556_686_912,
        // 48 layers x 4 KV heads x 128 head dim.
        kv_elems_per_token: KvElemsPerToken {
            k: 24_576,
            v: 24_576,
        },
        context: 32_768,
        trained_ctx: 40_960,
        rationale: "mixture-of-experts: 30B of knowledge, ~3B active per token, \
                    so it answers at roughly 3B speed",
    },
    ModelCandidate {
        repo: "bartowski/Qwen2.5-Coder-14B-Instruct-GGUF",
        quantization: Some("Q4_K_M"),
        weights_bytes: 8_988_111_072,
        // 48 layers x 8 KV heads x 128 head dim.
        kv_elems_per_token: KvElemsPerToken {
            k: 49_152,
            v: 49_152,
        },
        context: 32_768,
        trained_ctx: 32_768,
        rationale: "the strongest dense coding model that still leaves room for \
                    a 32k context",
    },
    ModelCandidate {
        repo: "bartowski/Qwen2.5-Coder-7B-Instruct-GGUF",
        quantization: Some("Q4_K_M"),
        weights_bytes: 4_683_074_336,
        // 28 layers x 4 KV heads x 128 head dim.
        kv_elems_per_token: KvElemsPerToken {
            k: 14_336,
            v: 14_336,
        },
        context: 32_768,
        trained_ctx: 32_768,
        rationale: "dependable tool calling from a mid-sized model",
    },
    ModelCandidate {
        repo: "bartowski/Qwen2.5-Coder-3B-Instruct-GGUF",
        quantization: Some("Q4_K_M"),
        weights_bytes: 1_929_903_360,
        // 36 layers x 2 KV heads x 128 head dim.
        kv_elems_per_token: KvElemsPerToken { k: 9_216, v: 9_216 },
        context: 32_768,
        trained_ctx: 32_768,
        rationale: "the smallest model in the list that still calls tools \
                    reliably enough to drive an agent",
    },
];

/// The phone shortlist, largest first.
///
/// Small models for plain chat on a phone, so the module's tool-call bias does
/// not decide this table (Gemma 3's template has no tool calls at all). Every
/// repository is public, so none needs a Hugging Face token.
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
/// the revision its row names; KV shapes and trained contexts are read from
/// each file's own GGUF metadata, the shapes counted as
/// [`estimate_kv_shape`](crate::domain::estimate_kv_shape)
/// counts them.
pub static PHONE_SHORTLIST: &[ModelCandidate] = &[
    ModelCandidate {
        repo: "unsloth/Qwen3-4B-GGUF",
        quantization: Some("Q4_K_M"),
        // Revision 22c9fc8a8c7700b76a1789366280a6a5a1ad1120.
        weights_bytes: 2_497_281_312,
        // 36 layers x 8 KV heads x 128 head dim.
        kv_elems_per_token: KvElemsPerToken {
            k: 36_864,
            v: 36_864,
        },
        context: 8192,
        trained_ctx: 40_960,
        rationale: "the strongest model here a phone can hold, and it thinks \
                    before it answers",
    },
    ModelCandidate {
        repo: "bartowski/Llama-3.2-3B-Instruct-GGUF",
        quantization: Some("Q4_K_M"),
        // Revision 5ab33fa94d1d04e903623ae72c95d1696f09f9e8.
        weights_bytes: 2_019_377_696,
        // 28 layers x 8 KV heads x 128 head dim.
        kv_elems_per_token: KvElemsPerToken {
            k: 28_672,
            v: 28_672,
        },
        context: 8192,
        trained_ctx: 131_072,
        rationale: "Meta's small chat model: it answers straight away, with no \
                    thinking to wait through",
    },
    ModelCandidate {
        repo: "unsloth/Qwen3-1.7B-GGUF",
        quantization: Some("Q4_K_M"),
        // Revision d7f544eead698dbd1f15126ef60b45a1e1933222.
        weights_bytes: 1_107_409_472,
        // 28 layers x 8 KV heads x 128 head dim.
        kv_elems_per_token: KvElemsPerToken {
            k: 28_672,
            v: 28_672,
        },
        context: 8192,
        trained_ctx: 40_960,
        rationale: "Qwen3's thinking in about half the memory of the 4B",
    },
    ModelCandidate {
        repo: "bartowski/Llama-3.2-1B-Instruct-GGUF",
        quantization: Some("Q4_K_M"),
        // Revision 067b946cf014b7c697f3654f621d577a3e3afd1c.
        weights_bytes: 807_694_464,
        // 16 layers x 8 KV heads x 64 head dim.
        kv_elems_per_token: KvElemsPerToken { k: 8192, v: 8192 },
        context: 8192,
        trained_ctx: 131_072,
        rationale: "Meta's smallest Llama 3.2: quick, direct answers in under a \
                    gigabyte",
    },
    ModelCandidate {
        repo: "unsloth/gemma-3-1b-it-GGUF",
        quantization: Some("Q4_K_M"),
        // Revision f0b45be0aac41bd6a100a4b5734cad5f67255bfb.
        weights_bytes: 806_058_272,
        // 26 layers x 1 KV head x 256 head dim. The file says
        // `sliding_window = 512` and has no `full_attention_interval`, so every
        // layer is counted at the full context: more than the sliding layers
        // keep, which is the safe direction.
        kv_elems_per_token: KvElemsPerToken { k: 6656, v: 6656 },
        context: 8192,
        trained_ctx: 32_768,
        rationale: "Google's smallest Gemma 3: short, plain answers in under a \
                    gigabyte",
    },
    ModelCandidate {
        repo: "unsloth/Qwen3-0.6B-GGUF",
        quantization: Some("Q4_K_M"),
        // Revision 50968a4468ef4233ed78cd7c3de230dd1d61a56b.
        weights_bytes: 396_705_472,
        // 28 layers x 8 KV heads x 128 head dim.
        kv_elems_per_token: KvElemsPerToken {
            k: 28_672,
            v: 28_672,
        },
        context: 8192,
        trained_ctx: 40_960,
        rationale: "the smallest model here, and it still thinks before it \
                    answers",
    },
];

/// What a launch on this machine is told beside the model: the KV cache
/// types it sizes a context's cache at, and the user's
/// `default-context-size`, which outranks the fit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Launching {
    /// The KV cache types a launch uses when none is asked for.
    pub kv_types: (KvCacheType, KvCacheType),
    /// The user's `default-context-size`, when set.
    pub default_ctx: Option<u64>,
}

impl Launching {
    /// The defaults: gglib's KV cache types, and no `default-context-size`.
    pub const DEFAULT: Self = Self {
        kv_types: (DEFAULT_CACHE_TYPE_K, DEFAULT_CACHE_TYPE_V),
        default_ctx: None,
    };
}

/// The least context a row is suggested at when none reaches the context
/// it is sized for: the ladder's second rung. A model served only the
/// lowest is too short of context to suggest.
const FALLBACK_FLOOR: u64 = SECOND_RUNG;

/// Recommend the largest model on `gglib up`'s shortlist that the desktop
/// load ([`fit_on_device`]) would serve at the context its row is sized for,
/// against `memory`; [`recommend_from`] says how.
#[must_use]
pub fn recommend(memory: SizedAgainst, launching: Launching) -> Option<Recommendation> {
    recommend_from(SHORTLIST, memory, launching, fit_on_device)
}

/// Recommend the largest model in `list` that `rule`, the fit its load makes,
/// serves at least the context its row is sized for inside `memory`'s
/// budget, at `launching`'s KV cache types.
///
/// `list` is ordered largest-first, as both tables are, so the first fit is
/// the best fit. The desktop load's fit is not monotonic in the budget (a
/// row can fit less a second slot's room at a lower rung, and be served
/// there), so each row is asked, not compared with a least budget.
///
/// When no row reaches its context, the row a load would serve the most
/// context is suggested, the larger on a tie, provided that is at least
/// [`FALLBACK_FLOOR`]; its sentence says the context it gets. With a
/// `default-context-size` set a launch serves that, and the sentence says
/// so, as a load's does.
///
/// Returns `None` when no row is served even [`FALLBACK_FLOOR`]. That is a
/// real answer, not a failure: suggesting a model that overflows would produce
/// a working-but-unusably-slow endpoint, and the user is better served by
/// being told their budget and left to choose.
#[must_use]
pub fn recommend_from(
    list: &'static [ModelCandidate],
    memory: SizedAgainst,
    launching: Launching,
    rule: FitRule,
) -> Option<Recommendation> {
    let fits: Vec<(&'static ModelCandidate, Option<u64>, FitInputs)> = list
        .iter()
        .map(|candidate| {
            let (fitted, inputs) = candidate.fit_by(rule, launching, memory.budget_bytes);
            (candidate, fitted, inputs)
        })
        .collect();
    let at_its_context = fits
        .iter()
        .find(|(candidate, fitted, _)| fitted.is_some_and(|ctx| ctx >= candidate.context));
    // Largest first, so a later row replaces the best only with more
    // context: on a tie the larger model stays.
    let most_context = || {
        fits.iter()
            .filter(|(_, fitted, _)| fitted.is_some_and(|ctx| ctx >= FALLBACK_FLOOR))
            .fold(
                None,
                |best: Option<&(_, Option<u64>, FitInputs)>, row| match best {
                    Some(kept) if kept.1 >= row.1 => Some(kept),
                    _ => Some(row),
                },
            )
    };
    let &(candidate, fitted, mut inputs) = at_its_context.or_else(most_context)?;
    inputs.system_ram_bytes = memory.system_ram_bytes;
    let (serves_at, source) = match launching.default_ctx {
        Some(ctx) => (ctx, ContextSizeSource::GlobalDefault),
        None => (fitted?, ContextSizeSource::FittedToHardware),
    };
    Some(Recommendation {
        candidate,
        budget_bytes: memory.budget_bytes,
        budget_source: memory.source,
        headroom_bytes: memory
            .budget_bytes
            .saturating_sub(candidate.required_bytes()),
        fit: ContextFit::new(serves_at, source, fitted, &inputs),
    })
}

#[cfg(test)]
#[path = "recommendation_tests.rs"]
mod tests;
