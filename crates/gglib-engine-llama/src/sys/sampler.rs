//! llama-server's sampler chain, built for one generation.

use std::ptr::NonNull;
use std::sync::Arc;

use super::{
    Context, Model, RawSampler, gglib_llama_sampler_accept, gglib_llama_sampler_free,
    gglib_llama_sampler_new, gglib_llama_sampler_sample, last_error,
};
use crate::decoder::{DecoderError, Sampling, Token};

/// llama-server's sampler chain for one generation, freed when dropped.
pub(crate) struct Sampler {
    raw: NonNull<RawSampler>,
    _model: Arc<Model>,
}

impl Sampler {
    /// The chain `s` asks for over `model`'s vocabulary, or llama-server's
    /// words for a value it refuses.
    #[expect(
        unsafe_code,
        reason = "the model is live; every other argument is a scalar"
    )]
    pub(crate) fn new(model: &Arc<Model>, s: &Sampling) -> Result<Self, DecoderError> {
        // SAFETY: the model is live.
        let raw = unsafe {
            gglib_llama_sampler_new(
                model.0.as_ptr(),
                s.temperature,
                s.top_k,
                s.top_p,
                s.min_p,
                s.typical_p,
                s.top_n_sigma,
                s.dynatemp_range,
                s.dynatemp_exponent,
                s.repeat_penalty,
                s.repeat_last_n,
                s.presence_penalty,
                s.frequency_penalty,
                s.dry_multiplier,
                s.dry_base,
                s.dry_allowed_length,
                s.dry_penalty_last_n,
                s.xtc_probability,
                s.xtc_threshold,
                s.mirostat,
                s.mirostat_tau,
                s.mirostat_eta,
                s.min_keep,
                s.seed,
                s.ignore_eos,
            )
        };
        let raw = NonNull::new(raw)
            .ok_or_else(|| DecoderError::Decode(last_error("llama.cpp made no sampler chain")))?;
        Ok(Self {
            raw,
            _model: Arc::clone(model),
        })
    }

    /// `token` counted in the chain's history, as a prompt's tokens are.
    #[expect(unsafe_code, reason = "the sampler is live")]
    pub(crate) fn accept(&mut self, token: Token) {
        // SAFETY: the sampler is live.
        unsafe { gglib_llama_sampler_accept(self.raw.as_ptr(), token) }
    }

    /// The next token, from the logits of the last token `ctx` decoded.
    #[expect(unsafe_code, reason = "the sampler and the context are live")]
    pub(crate) fn sample(&mut self, ctx: &mut Context) -> Token {
        // SAFETY: both are live, and ctx was made from the sampler's model.
        unsafe { gglib_llama_sampler_sample(self.raw.as_ptr(), ctx.raw.as_ptr()) }
    }
}

impl Drop for Sampler {
    #[expect(unsafe_code, reason = "frees the sampler this handle owns, once")]
    fn drop(&mut self) {
        // SAFETY: the sampler is live and freed here only.
        unsafe { gglib_llama_sampler_free(self.raw.as_ptr()) }
    }
}
