#![doc = include_str!("README.md")]
pub(crate) mod decoder;
pub(crate) mod encoder;
pub(crate) mod frames;
pub mod parser;

pub use decoder::SseStreamDecoder;
pub use encoder::{DONE_SENTINEL, SseEncoder};
pub use frames::DataFrames;
pub use parser::{SseParseResult, parse_sse_frame};
