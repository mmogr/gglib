#![doc = include_str!("README.md")]
pub mod error;
pub mod history;
pub mod parser;
pub mod parsers;
pub mod registry;
pub mod stream;
pub mod tags;

pub use error::{NormalizationError, NormalizationErrorKind};
pub use history::strip_thinking_debt;
pub use parser::{ParserOutput, ToolCallParser};
pub use registry::get_parser;
pub use stream::NormalizingStream;
