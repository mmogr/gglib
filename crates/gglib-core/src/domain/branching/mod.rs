#![doc = include_str!("README.md")]

mod plan;
mod points;
mod preview;
mod units;
mod wire;

pub use plan::{EDITED_KEY, Plan, Refused, Then, answerable, plan};
pub use points::{LineChat, LineRow, points};
pub use preview::{PREVIEW_CHARS, PreviewRow, preview};
pub use units::{Unit, units};
pub use wire::{BranchOption, BranchPoint, ChatChange, ChatChanged, ChatThread};

#[cfg(test)]
mod fixture;

#[cfg(test)]
#[path = "plan_tests.rs"]
mod plan_tests;

#[cfg(test)]
#[path = "points_tests.rs"]
mod points_tests;

#[cfg(test)]
#[path = "preview_tests.rs"]
mod preview_tests;

#[cfg(test)]
#[path = "wire_tests.rs"]
mod wire_tests;

#[cfg(test)]
#[path = "contract_tests.rs"]
mod contract_tests;
