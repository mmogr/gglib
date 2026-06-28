#![doc = include_str!("README.md")]
pub mod decoder;
pub mod encoder;
pub mod parser;

pub use decoder::SseStreamDecoder;
pub use encoder::SseEncoder;
pub use parser::{SseParseResult, parse_sse_frame};
