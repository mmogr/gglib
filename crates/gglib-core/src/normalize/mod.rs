#![doc = include_str!("README.md")]
pub(crate) mod history;
pub mod residue;
pub mod tags;

pub use history::strip_thinking_debt;
pub use residue::{ResidueScanner, scan_complete};
