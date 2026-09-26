#![doc = include_str!("README.md")]
mod lease;
#[allow(
    clippy::cast_possible_truncation,
    reason = "a wait's milliseconds pass u64::MAX only after 584 million years"
)]
#[allow(
    clippy::unused_self,
    reason = "grandfathered at lint inheritance, #1157"
)]
mod state;

pub use lease::AdmissionQueue;
pub(crate) use state::launch_timeout;
pub use state::{
    ADMISSION_DEADLINE, AdmissionDecision, DRAIN_QUANTUM, PRIMARY_SLOT, Resident, SLOT_COUNT,
    SlotState, Ticket,
};

#[cfg(test)]
#[path = "queue_tests.rs"]
#[allow(
    clippy::unchecked_time_subtraction,
    reason = "grandfathered at lint inheritance, #1157"
)]
mod queue_tests;

/// What eviction does to a slot that is not a plain resident.
#[cfg(test)]
#[path = "state_tests.rs"]
mod state_tests;
