//! What setup suggests for this machine, and the budget a download that
//! names no size is judged against: both read off the machine as detected,
//! the detection [`super::fit_memory()`] reads too.
//!
//! Pure functions of what was detected (`*_on`), so every shape of machine
//! is asserted on any one, and the same answers for this machine.

use gglib_core::domain::recommendation::{Recommendation, recommend};
use gglib_core::utils::system::SystemMemoryInfo;

/// The memory a download that names no size is judged against.
///
/// The size picker's verdicts are judged against it too. On a machine with
/// `gpu_present` and `mem`: the GPU's, or else the machine's whole RAM.
/// `None` when neither was read.
#[must_use]
pub fn download_budget_on(gpu_present: bool, mem: &SystemMemoryInfo) -> Option<u64> {
    super::fit_memory_of(gpu_present, mem)
        .device_bytes()
        .or_else(|| (mem.total_ram_bytes > 0).then_some(mem.total_ram_bytes))
}

/// The model setup suggests for a machine with `gpu_present` and `mem`.
#[must_use]
pub fn recommend_on(_gpu_present: bool, mem: &SystemMemoryInfo) -> Option<Recommendation> {
    recommend(mem)
}

/// [`download_budget_on`] this machine. The first call probes the hardware.
#[must_use]
pub fn download_budget() -> Option<u64> {
    let machine = super::detected();
    download_budget_on(machine.gpu_present, &machine.memory)
}

/// [`recommend_on`] this machine. The first call probes the hardware.
#[must_use]
pub fn recommend_here() -> Option<Recommendation> {
    let machine = super::detected();
    recommend_on(machine.gpu_present, &machine.memory)
}
