#![doc = include_str!("README.md")]
mod gate;
mod hold;
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
mod timing;

pub use lease::AdmissionQueue;
pub use state::{
    AdmissionDecision, Candidate, PRIMARY_SLOT, Refusal, Resident, SLOT_COUNT, SlotState, Ticket,
};
pub(crate) use timing::launch_timeout;
pub use timing::{ADMISSION_DEADLINE, DRAIN_QUANTUM};

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

/// Where an image model is placed, and what a held slot does to the line.
#[cfg(test)]
#[path = "state_placement_tests.rs"]
mod state_placement_tests;
