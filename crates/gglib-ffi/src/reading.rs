//! A reading of the device's memory, as the app takes it.
//!
//! The app samples its device and gglib makes the budget from the sample
//! (`gglib_core::domain::device_budget`), so the rule lives in gglib and the
//! app never sizes anything itself.

use gglib_core::domain::device_budget::{self, budget_from};

/// One reading of the device's memory, taken by the app.
///
/// It is taken once each time the app opens a model browser, and held for as
/// long as that browser's verdicts are on screen, and once each time it
/// serves the engine, and held for the whole serve.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct DeviceReading {
    /// The device's physical memory, in bytes.
    pub physical_bytes: u64,
    /// The memory the app's process can still allocate, in bytes.
    pub available_bytes: u64,
    /// The reading was taken on a simulator, whose memory is the host's.
    pub is_simulator: bool,
}

impl DeviceReading {
    /// The budget a model may spend on this reading's device, or `None` for
    /// a reading that failed (a zero in it).
    pub(crate) fn budget(&self) -> Option<u64> {
        budget_from(&self.for_core())
    }

    /// The same reading as gglib-core takes it.
    pub(crate) const fn for_core(self) -> device_budget::DeviceReading {
        device_budget::DeviceReading {
            physical_bytes: self.physical_bytes,
            available_bytes: self.available_bytes,
            is_simulator: self.is_simulator,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GIB: u64 = 1 << 30;

    #[test]
    fn the_budget_is_gglibs_rule_over_the_reading() {
        let reading = DeviceReading {
            physical_bytes: 8 * GIB,
            available_bytes: 6 * GIB,
            is_simulator: false,
        };

        assert_eq!(
            reading.budget(),
            budget_from(&device_budget::DeviceReading {
                physical_bytes: 8 * GIB,
                available_bytes: 6 * GIB,
                is_simulator: false,
            })
        );
        assert!(reading.budget().is_some_and(|budget| budget < 6 * GIB));
    }

    #[test]
    fn a_reading_that_failed_gives_no_budget() {
        let failed = DeviceReading {
            physical_bytes: 8 * GIB,
            available_bytes: 0,
            is_simulator: false,
        };

        assert_eq!(failed.budget(), None);
    }
}
