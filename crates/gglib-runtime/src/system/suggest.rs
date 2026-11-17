//! What setup suggests for this machine, and the budget a download that
//! names no size is judged against: both the load's own reading of the
//! machine ([`fit_memory_of`](super::fit_memory_of)), so a size either calls
//! fitting is one a load fits.
//!
//! Pure functions of what was detected (`*_on`), so every shape of machine
//! is asserted on any one, and the same answers for this machine, from the
//! detection [`super::fit_memory()`] reads.

use gglib_core::cache_config::launch_kv_types;
use gglib_core::domain::recommendation::{
    BudgetSource, Launching, Recommendation, SizedAgainst, recommend,
};
use gglib_core::utils::system::SystemMemoryInfo;

use super::FitMemory;

/// Why nothing is suggested to a machine whose GPU memory could not be
/// read, and what to do: the words `gglib up` and the setup step both say.
pub const UNREADABLE_GPU: &str = "gglib could not read this GPU's memory, so it cannot size a \
                                  model to it and suggests none: choose one yourself.";

/// The memory a download that names no size is judged against.
///
/// The size picker's verdicts are judged against it too. It is the load's
/// budget on a machine with `gpu_present` and `mem`: a GPU's memory, or with
/// no GPU the share of RAM the load spends. `None` when the load has none (a
/// GPU whose memory could not be read): the download then takes the smallest
/// size, and the picker says the machine's memory could not be read.
#[must_use]
pub fn download_budget_on(gpu_present: bool, mem: &SystemMemoryInfo) -> Option<u64> {
    super::fit_memory_of(gpu_present, mem).budget_bytes()
}

/// The load's budget on a machine with `gpu_present` and `mem`, as the
/// suggestion is told it; `None` when the load has none.
#[must_use]
pub fn sized_against_on(gpu_present: bool, mem: &SystemMemoryInfo) -> Option<SizedAgainst> {
    match super::fit_memory_of(gpu_present, mem) {
        FitMemory::Device { bytes } => Some(SizedAgainst {
            budget_bytes: bytes,
            source: if mem.is_unified_memory {
                BudgetSource::UnifiedMemory
            } else {
                BudgetSource::Vram
            },
            system_ram_bytes: None,
        }),
        FitMemory::SystemRam {
            total_bytes,
            budget_bytes,
        } => Some(SizedAgainst {
            budget_bytes,
            source: BudgetSource::SystemRam,
            system_ram_bytes: Some(total_bytes),
        }),
        FitMemory::Unreadable => None,
    }
}

/// What a launch on this machine is told beside the model: the KV cache
/// types it uses when none is asked for, and the user's
/// `default-context-size`.
#[must_use]
pub fn launching(default_ctx: Option<u64>) -> Launching {
    Launching {
        kv_types: launch_kv_types(),
        default_ctx,
    }
}

/// The model setup suggests for a machine with `gpu_present` and `mem`.
///
/// Told what a launch is told ([`launching`]), sized against the load's
/// budget ([`sized_against_on`]); `None` when nothing on the list is served
/// 8K, or the load has no budget to fit by.
#[must_use]
pub fn recommend_with(
    gpu_present: bool,
    mem: &SystemMemoryInfo,
    launching: Launching,
) -> Option<Recommendation> {
    recommend(sized_against_on(gpu_present, mem)?, launching)
}

/// [`recommend_with`] with no `default-context-size` set.
#[must_use]
pub fn recommend_on(gpu_present: bool, mem: &SystemMemoryInfo) -> Option<Recommendation> {
    recommend_with(gpu_present, mem, launching(None))
}

/// Why nothing is suggested to a machine with `gpu_present` and `mem`, when
/// the reason is not that nothing fits: [`UNREADABLE_GPU`] when the load has
/// no budget. `None` otherwise.
#[must_use]
pub fn no_suggestion_reason_on(gpu_present: bool, mem: &SystemMemoryInfo) -> Option<&'static str> {
    sized_against_on(gpu_present, mem)
        .is_none()
        .then_some(UNREADABLE_GPU)
}

/// [`download_budget_on`] this machine. The first call probes the hardware.
#[must_use]
pub fn download_budget() -> Option<u64> {
    let machine = super::detected();
    download_budget_on(machine.gpu_present, &machine.memory)
}

/// [`sized_against_on`] this machine.
#[must_use]
pub fn sized_against() -> Option<SizedAgainst> {
    let machine = super::detected();
    sized_against_on(machine.gpu_present, &machine.memory)
}

/// [`recommend_with`] this machine, with the user's `default_ctx`.
#[must_use]
pub fn recommend_here(default_ctx: Option<u64>) -> Option<Recommendation> {
    let machine = super::detected();
    recommend_with(machine.gpu_present, &machine.memory, launching(default_ctx))
}

/// [`no_suggestion_reason_on`] this machine.
#[must_use]
pub fn no_suggestion_reason() -> Option<&'static str> {
    let machine = super::detected();
    no_suggestion_reason_on(machine.gpu_present, &machine.memory)
}

#[cfg(test)]
#[path = "suggest_tests.rs"]
mod tests;
