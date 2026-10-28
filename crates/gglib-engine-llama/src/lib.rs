#![doc = include_str!(concat!(env!("OUT_DIR"), "/README_GENERATED.md"))]

mod decoder;
mod engine;
mod render;
mod runtime;
pub mod shim;
mod state;
#[cfg(gglib_llama)]
mod sys;

#[cfg(gglib_llama)]
pub use decoder::LlamaDecoder;
pub use decoder::{
    Abort, Decoder, DecoderError, FinishReason, Finished, LoadParams, Loaded, PromptProgress,
    Sampling, Step, Token,
};
#[cfg(any(test, feature = "test-decoder"))]
pub use decoder::{Script, ScriptRecord, ScriptedDecoder};
pub use engine::Engine;
pub use runtime::InProcessRuntime;
pub use state::EngineState;
