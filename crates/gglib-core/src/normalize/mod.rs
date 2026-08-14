#![doc = include_str!("README.md")]
pub(crate) mod coerce;
pub(crate) mod error;
pub(crate) mod history;
pub(crate) mod oneshot;
pub(crate) mod parser;
pub(crate) mod parsers;
pub mod registry;
pub mod residue;
pub(crate) mod stream;
pub mod tags;

pub use error::{NormalizationError, NormalizationErrorKind};
pub use history::strip_thinking_debt;
pub use oneshot::normalize_chat_completion_body;
pub use parser::{ParserOutput, ToolCallParser};
pub use registry::get_parser;
pub use residue::{ResidueScanner, scan_complete};
pub use stream::NormalizingStream;
