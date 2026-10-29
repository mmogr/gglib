#![doc = include_str!("README.md")]

mod abort;
#[cfg(gglib_llama)]
mod llama;
mod sampling;
#[cfg(any(test, feature = "test-decoder"))]
mod scripted;
#[cfg(any(test, gglib_llama))]
mod utf8;

use std::path::Path;

use gglib_core::cache_config::KvCacheType;
use gglib_gguf::VocabTokens;

pub use abort::Abort;
#[cfg(gglib_llama)]
pub use llama::LlamaDecoder;
pub use sampling::Sampling;
#[cfg(any(test, feature = "test-decoder"))]
pub use scripted::{Script, ScriptRecord, ScriptedDecoder};

/// A token of the loaded model's vocabulary, as llama.h numbers it.
pub type Token = i32;

/// What a model is loaded with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LoadParams {
    /// The context to create, in tokens.
    pub n_ctx: u32,
    /// The type the KV cache holds keys in.
    pub cache_type_k: KvCacheType,
    /// The type the KV cache holds values in.
    pub cache_type_v: KvCacheType,
}

/// A model a decoder holds, and what the chat it serves needs of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Loaded {
    /// The context the model was trained with, from its GGUF.
    pub n_ctx_train: u32,
    /// The context the decoder created, which a prompt and its reply share.
    pub n_ctx: u32,
    /// The model's `tokenizer.chat_template`, empty when it carries none.
    pub chat_template: String,
    /// The vocabulary's special tokens, as the template is given them.
    pub vocab: VocabTokens,
}

/// How far a prompt has been read, as llama-server's `prompt_progress`
/// counts it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PromptProgress {
    /// The prompt's tokens.
    pub total: u32,
    /// The prompt's leading tokens the KV cache already held.
    pub cache: u32,
    /// The tokens read so far, the cached ones included.
    pub processed: u32,
}

/// One step of a generation, as the decoder hands it out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// The prompt was read up to here.
    Progress(PromptProgress),
    /// A sampled token's text. A token that ends inside a character
    /// carries what completes so far, possibly nothing; an end-of-generation
    /// token is not a piece.
    Piece(String),
}

/// Why a generation ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FinishReason {
    /// The model sampled an end-of-generation token.
    Eos,
    /// `n_predict` tokens were generated, or the context is full.
    Length,
    /// The [`Abort`] was raised.
    Aborted,
}

/// The counts a generation ends with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Finished {
    /// Why it ended.
    pub reason: FinishReason,
    /// The prompt's leading tokens taken from the KV cache rather than read.
    pub cached_tokens: u32,
    /// The tokens sampled, the end-of-generation token included.
    pub completion_tokens: u32,
}

/// What went wrong in a decoder.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DecoderError {
    /// The model could not be loaded.
    #[error("loading the model failed: {0}")]
    Load(String),
    /// There is no model to work with.
    #[error("no model is loaded")]
    NotLoaded,
    /// Text could not be turned into tokens.
    #[error("tokenizing failed: {0}")]
    Tokenize(String),
    /// A decode step failed.
    #[error("decoding failed: {0}")]
    Decode(String),
    /// A generation was asked of an empty prompt, which leaves nothing to
    /// sample a reply after.
    #[error("the prompt is empty, so there is nothing to sample a reply after")]
    EmptyPrompt,
    /// The load's [`Abort`] was raised before the model was ready, and no
    /// model is held.
    #[error("the load was stopped before the model was ready")]
    LoadAborted,
}

/// llama.cpp behind a seam: one model at a time, one generation at a time.
///
/// Every call blocks: the engine runs [`Decoder::generate`] on a thread of
/// its own, and whoever loads and unloads models through it does so from a
/// thread that may block.
pub trait Decoder: Send {
    /// Load the model at `path`, freeing any model held before, handing
    /// `progress` the fraction read, from 0 to 1, as the load goes on.
    ///
    /// `abort` is checked at every report: once it is raised the load stops
    /// there, frees what it read, and is [`DecoderError::LoadAborted`].
    fn load(
        &mut self,
        path: &Path,
        params: &LoadParams,
        abort: &Abort,
        progress: &mut dyn FnMut(f32),
    ) -> Result<Loaded, DecoderError>;

    /// `text` as tokens, the vocabulary's BOS added when it adds one and
    /// special tokens read as themselves, as llama-server tokenizes a
    /// rendered prompt.
    fn tokenize(&self, text: &str) -> Result<Vec<Token>, DecoderError>;

    /// Read `prompt` and sample a reply under `sampling`, handing each
    /// [`Step`] to `sink` as it happens.
    ///
    /// The leading tokens the previous generation left in the KV cache are
    /// kept rather than read again, as llama-server's `cache_prompt` does.
    /// Generation ends at an end-of-generation token, at
    /// `sampling.n_predict` tokens, when the context is full, or when
    /// `abort` is raised: it is checked before every decode step, so a raised
    /// abort ends the generation within one step.
    ///
    /// An empty `prompt` is refused as [`DecoderError::EmptyPrompt`] before
    /// the KV cache is touched: llama.cpp has no logits to sample from until
    /// a token is read. The engine answers an empty prompt itself.
    fn generate(
        &mut self,
        prompt: &[Token],
        sampling: &Sampling,
        abort: &Abort,
        sink: &mut dyn FnMut(Step),
    ) -> Result<Finished, DecoderError>;

    /// Free the model and its context. Nothing to free is not an error.
    fn unload(&mut self);
}

/// A boxed decoder is a decoder, so a caller that picks one at run time, as
/// [`linked_llama_decoder`] hands it, starts an engine over it as over any
/// other.
impl<D: Decoder + ?Sized> Decoder for Box<D> {
    fn load(
        &mut self,
        path: &Path,
        params: &LoadParams,
        abort: &Abort,
        progress: &mut dyn FnMut(f32),
    ) -> Result<Loaded, DecoderError> {
        (**self).load(path, params, abort, progress)
    }

    fn tokenize(&self, text: &str) -> Result<Vec<Token>, DecoderError> {
        (**self).tokenize(text)
    }

    fn generate(
        &mut self,
        prompt: &[Token],
        sampling: &Sampling,
        abort: &Abort,
        sink: &mut dyn FnMut(Step),
    ) -> Result<Finished, DecoderError> {
        (**self).generate(prompt, sampling, abort, sink)
    }

    fn unload(&mut self) {
        (**self).unload();
    }
}

/// `LlamaDecoder` as a [`Decoder`], offloading `n_gpu_layers` layers when
/// given (-1 for every one), when this build links llama.framework; `None`
/// when it does not.
///
/// `LlamaDecoder` exists only when the `llama` feature compiled the C shim
/// and linked the framework: an Apple target with `GGLIB_LLAMA_XCFRAMEWORK`
/// set. A crate that turns the feature on for a build that may link nothing,
/// a test built on any OS among them, asks here rather than naming the type.
#[cfg(gglib_llama)]
pub fn linked_llama_decoder(n_gpu_layers: Option<i32>) -> Option<Box<dyn Decoder>> {
    let decoder = LlamaDecoder::new();
    Some(Box::new(match n_gpu_layers {
        Some(n) => decoder.with_gpu_layers(n),
        None => decoder,
    }))
}

/// `LlamaDecoder` as a [`Decoder`], offloading `n_gpu_layers` layers when
/// given (-1 for every one), when this build links llama.framework; `None`
/// when it does not, as here.
///
/// `LlamaDecoder` exists only when the `llama` feature compiled the C shim
/// and linked the framework: an Apple target with `GGLIB_LLAMA_XCFRAMEWORK`
/// set. A crate that turns the feature on for a build that may link nothing,
/// a test built on any OS among them, asks here rather than naming the type.
#[cfg(not(gglib_llama))]
pub const fn linked_llama_decoder(_n_gpu_layers: Option<i32>) -> Option<Box<dyn Decoder>> {
    None
}
