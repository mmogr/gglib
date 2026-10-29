#![doc = include_str!(concat!(env!("OUT_DIR"), "/README_GENERATED.md"))]

mod decoder;
mod engine;
mod render;
pub mod shim;

pub use decoder::{
    Abort, Decoder, DecoderError, FinishReason, Finished, LoadParams, Loaded, PromptProgress,
    Sampling, Step, Token,
};
#[cfg(any(test, feature = "test-decoder"))]
pub use decoder::{Script, ScriptRecord, ScriptedDecoder};
pub use engine::Engine;
