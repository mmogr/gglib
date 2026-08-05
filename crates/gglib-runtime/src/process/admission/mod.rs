#![doc = include_str!("README.md")]
mod lease;
mod state;

pub use lease::AdmissionQueue;
pub use state::{
    ADMISSION_DEADLINE, AdmissionDecision, DRAIN_QUANTUM, LAUNCH_TIMEOUT, PRIMARY_SLOT, Resident,
    SLOT_COUNT, SlotState, Ticket,
};

#[cfg(test)]
#[path = "queue_tests.rs"]
mod queue_tests;
