//! A device's memory budget for a model, from one reading of its memory.
//!
//! The device reads its memory and Rust makes the budget from it, so the rule
//! lives in one place and a caller holds a number, never a rule of its own.
//!
//! The budget is read once per serve and held: every admission and every fit
//! for that serve is made against the same number. The fitted context ends up
//! in a resident's identity, so a budget taken afresh for each request would
//! move with whatever else the device is doing and relaunch the model it just
//! sized (`context_fit`, "Why it snaps to rungs", and `fit_context`'s
//! `budget_bytes`).
//!
//! [`budget_from`] gives the whole budget. The fit spends
//! [`BUDGET_UTILISATION`](crate::domain::BUDGET_UTILISATION) of it
//! ([`fit_context`](crate::domain::fit_context) applies that share itself), so
//! it is not taken off here as well.

/// The share of a device's physical memory a model may be given, in percent.
/// No device reading backs this value yet.
pub const PHONE_SHARE_PERCENT: u64 = 60;

/// One reading of a device's memory, taken by the device.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeviceReading {
    /// The device's physical memory, in bytes.
    pub physical_bytes: u64,
    /// The memory this process can still allocate, in bytes.
    pub available_bytes: u64,
    /// The reading was taken on a simulator, whose memory is the host's.
    pub is_simulator: bool,
}

/// The budget a model may spend on `reading`'s device: the smaller of the
/// memory still available and [`PHONE_SHARE_PERCENT`] of physical memory,
/// counted in whole hundredths of it.
///
/// `None` when either reading is zero: that is a reading that failed, not a
/// device with no memory.
#[must_use]
pub fn budget_from(reading: &DeviceReading) -> Option<u64> {
    if reading.physical_bytes == 0 || reading.available_bytes == 0 {
        return None;
    }
    Some(
        reading
            .available_bytes
            .min(reading.physical_bytes / 100 * PHONE_SHARE_PERCENT),
    )
}

#[cfg(test)]
#[path = "device_budget_tests.rs"]
mod tests;
