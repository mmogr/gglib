#![doc = include_str!("README.md")]
pub(crate) mod history;
pub mod residue;

pub use history::strip_thinking_debt;
pub use residue::{NoAnswer, ResidueScanner, no_answer, scan_complete};
