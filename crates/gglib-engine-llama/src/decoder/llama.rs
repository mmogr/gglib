//! The decoder over llama.h, through the C shim `crate::sys` wraps.

use std::path::Path;
use std::sync::Arc;

use gglib_gguf::VocabTokens;

use super::utf8::Utf8Pieces;
use super::{
    Abort, Decoder, DecoderError, FinishReason, Finished, LoadParams, Loaded, PromptProgress,
    Sampling, Step, Token,
};
use crate::sys::{self, Backend, Context, DecodeStop, Model, Sampler};

/// Prompt tokens read per decode step, and so per check of the abort and
/// per `prompt_progress` report: llama.cpp's default physical batch.
const N_BATCH: u32 = 512;

/// The layers [`LlamaDecoder::new`] offloads to the GPU: every one on an
/// iPhone, none on its simulator, whose Metal llama.cpp cannot use, and none
/// on a Mac, where [`LlamaDecoder::with_gpu_layers`] chooses.
const DEFAULT_GPU_LAYERS: i32 = if cfg!(all(
    target_os = "ios",
    not(target_abi = "sim"),
    not(target_arch = "x86_64")
)) {
    -1
} else {
    0
};

/// A [`Decoder`] over llama.cpp in this process: llama.framework at the
/// pin, reached through the C shim in `csrc/`.
///
/// A model's weights are memory-mapped. A context is made at the size and
/// KV cache types [`LoadParams`] names, with flash attention as llama.cpp
/// chooses it, which it turns on for a quantised V cache. A generation
/// samples with llama-server's chain at the pin, its abort checked before
/// each decode step and, on the CPU, inside llama.cpp's own decode too.
pub struct LlamaDecoder {
    held: Option<Held>,
    n_gpu_layers: i32,
    /// Dropped last, after the model and context it serves.
    _backend: Backend,
}

/// A model loaded, its context, and what its KV cache holds.
struct Held {
    ctx: Context,
    /// The tokens the KV cache holds, in order.
    cache: Vec<Token>,
    /// The sliding window of the model's attention, at least 1.
    n_swa: u32,
}

impl LlamaDecoder {
    /// A decoder holding no model, offloading every layer to the GPU on an
    /// iPhone and none on its simulator or a Mac.
    pub fn new() -> Self {
        Self {
            held: None,
            n_gpu_layers: DEFAULT_GPU_LAYERS,
            _backend: Backend::acquire(),
        }
    }

    /// The same decoder offloading `n_gpu_layers` layers, -1 for every one:
    /// for a Mac, whose GPU a caller knows to be there.
    #[must_use]
    pub fn with_gpu_layers(mut self, n_gpu_layers: i32) -> Self {
        self.n_gpu_layers = n_gpu_layers;
        self
    }
}

impl Default for LlamaDecoder {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for LlamaDecoder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LlamaDecoder")
            .field("loaded", &self.held.is_some())
            .field("n_gpu_layers", &self.n_gpu_layers)
            .finish_non_exhaustive()
    }
}

impl Decoder for LlamaDecoder {
    fn load(
        &mut self,
        path: &Path,
        params: &LoadParams,
        abort: &Abort,
        progress: &mut dyn FnMut(f32),
    ) -> Result<Loaded, DecoderError> {
        self.unload();
        if abort.is_raised() {
            return Err(DecoderError::LoadAborted);
        }
        let model = Arc::new(Model::load(path, self.n_gpu_layers, abort, progress)?);
        let ctx = Context::new(
            Arc::clone(&model),
            params.n_ctx,
            N_BATCH,
            None,
            params.cache_type_k.as_llama_arg(),
            params.cache_type_v.as_llama_arg(),
        )?;
        let facts = model.facts();
        // The template's bos_token and eos_token, as llama-server reads them
        // (common_chat_templates_init): the pieces with special tokens shown.
        let special = |token: Token| match token {
            -1 => String::new(),
            t => String::from_utf8_lossy(&model.piece(t, true)).into_owned(),
        };
        let loaded = Loaded {
            n_ctx_train: u32::try_from(facts.n_ctx_train).unwrap_or(0),
            n_ctx: ctx.n_ctx(),
            chat_template: model.chat_template(),
            vocab: VocabTokens {
                bos: special(facts.bos),
                eos: special(facts.eos),
                add_bos: facts.add_bos,
                add_eos: facts.add_eos,
            },
        };
        tracing::info!(
            path = %path.display(),
            n_ctx = loaded.n_ctx,
            n_ctx_train = loaded.n_ctx_train,
            n_gpu_layers = self.n_gpu_layers,
            gpu_offload = sys::supports_gpu_offload(),
            "llama.cpp loaded a model"
        );
        self.held = Some(Held {
            ctx,
            cache: Vec::new(),
            n_swa: u32::try_from(facts.n_swa).unwrap_or(0).max(1),
        });
        Ok(loaded)
    }

    fn tokenize(&self, text: &str) -> Result<Vec<Token>, DecoderError> {
        let held = self.held.as_ref().ok_or(DecoderError::NotLoaded)?;
        held.ctx.model().tokenize(text)
    }

    fn generate(
        &mut self,
        prompt: &[Token],
        sampling: &Sampling,
        abort: &Abort,
        sink: &mut dyn FnMut(Step),
    ) -> Result<Finished, DecoderError> {
        let held = self.held.as_mut().ok_or(DecoderError::NotLoaded)?;
        // Refused before the cache is cut. With no token read there are no
        // logits for this prompt: llama.cpp would abort the process, or
        // sample after the last reply's.
        if prompt.is_empty() {
            return Err(DecoderError::EmptyPrompt);
        }
        held.ctx.watch(abort);
        let total = len32(prompt.len());
        let cached = held.reuse(prompt);
        let mut finished = Finished {
            reason: FinishReason::Eos,
            cached_tokens: cached,
            completion_tokens: 0,
        };
        let progress = |processed| {
            Step::Progress(PromptProgress {
                total,
                cache: cached,
                processed,
            })
        };
        sink(progress(cached));
        for batch in prompt[held.cache.len()..].chunks(N_BATCH as usize) {
            if abort.is_raised() || held.decode(batch)? == Read::Aborted {
                finished.reason = FinishReason::Aborted;
                return Ok(finished);
            }
            sink(progress(len32(held.cache.len())));
        }
        finished.reason = held.sample(
            total,
            sampling,
            abort,
            sink,
            &mut finished.completion_tokens,
        )?;
        Ok(finished)
    }

    fn unload(&mut self) {
        self.held = None;
    }
}

/// How a decode the generation asked for ended.
#[derive(Debug, PartialEq, Eq)]
enum Read {
    /// Every token was read into the KV cache.
    All,
    /// The abort stopped it.
    Aborted,
}

impl Held {
    /// Keep the longest prefix the KV cache shares with `prompt`, at least
    /// its last token left to read, as llama-server's `cache_prompt` does;
    /// how many tokens were kept.
    ///
    /// A cache that cannot be cut there, or a sliding-window cache that no
    /// longer holds the window before the cut, is emptied and the prompt
    /// read whole, as llama-server reads it then.
    fn reuse(&mut self, prompt: &[Token]) -> u32 {
        let common = self
            .cache
            .iter()
            .zip(prompt)
            .take_while(|(held, asked)| held == asked)
            .count();
        let keep = len32(common.min(prompt.len().saturating_sub(1)));
        let window_held =
            i64::from(self.ctx.pos_min()) <= i64::from(keep.saturating_sub(self.n_swa));
        let keep = if keep > 0 && window_held && self.ctx.keep(keep) {
            keep
        } else {
            self.ctx.keep(0);
            0
        };
        self.cache.truncate(keep as usize);
        keep
    }

    /// `tokens` read after what the KV cache holds, the record of it kept
    /// in step: a decode that stops part of the way through is cut back to
    /// where it started.
    fn decode(&mut self, tokens: &[Token]) -> Result<Read, DecoderError> {
        match self.ctx.decode(tokens) {
            Ok(()) => {
                self.cache.extend_from_slice(tokens);
                Ok(Read::All)
            }
            Err(stop) => {
                if !self.ctx.keep(len32(self.cache.len())) {
                    self.ctx.keep(0);
                    self.cache.clear();
                }
                match stop {
                    DecodeStop::Aborted => Ok(Read::Aborted),
                    DecodeStop::Failed(why) => Err(DecoderError::Decode(why)),
                }
            }
        }
    }

    /// Sample a reply to the prompt of `total` tokens the KV cache holds,
    /// handing out each token's piece, counting each token sampled in
    /// `completion`, and saying why it ended.
    ///
    /// The chain is built for this generation and told the prompt first, as
    /// llama-server's slot tells its sampler. A sampled token is decoded only
    /// when the next one is wanted, so the KV cache never holds a token past
    /// the reply.
    fn sample(
        &mut self,
        total: u32,
        sampling: &Sampling,
        abort: &Abort,
        sink: &mut dyn FnMut(Step),
        completion: &mut u32,
    ) -> Result<FinishReason, DecoderError> {
        let model = Arc::clone(self.ctx.model());
        let mut sampler = Sampler::new(&model, sampling)?;
        for &token in &self.cache {
            sampler.accept(token);
        }
        let n_ctx = self.ctx.n_ctx();
        let limit = u32::try_from(sampling.n_predict).ok();
        let mut text = Utf8Pieces::default();
        let mut unread: Option<Token> = None;
        let reason = loop {
            if abort.is_raised() {
                break FinishReason::Aborted;
            }
            if limit.is_some_and(|n| *completion >= n) || total + *completion >= n_ctx {
                break FinishReason::Length;
            }
            if let Some(token) = unread.take()
                && self.decode(&[token])? == Read::Aborted
            {
                break FinishReason::Aborted;
            }
            let token = sampler.sample(&mut self.ctx);
            *completion += 1;
            if !sampling.ignore_eos && model.is_eog(token) {
                break FinishReason::Eos;
            }
            sink(Step::Piece(text.push(&model.piece(token, false))));
            unread = Some(token);
        };
        let rest = text.finish();
        if !rest.is_empty() {
            sink(Step::Piece(rest));
        }
        Ok(reason)
    }
}

fn len32(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

#[cfg(test)]
#[path = "llama_tests.rs"]
mod llama_tests;
