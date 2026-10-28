//! A context over a model: its KV cache, its decode, and the abort a decode
//! asks.

use std::ffi::{CString, c_void};
use std::ptr::NonNull;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use super::{
    Model, RawContext, gglib_llama_context_free, gglib_llama_context_new, gglib_llama_decode,
    gglib_llama_memory_keep, gglib_llama_memory_pos_min, gglib_llama_n_ctx, gglib_llama_set_abort,
    last_error,
};
use crate::decoder::{Abort, DecoderError, Token};

/// Why a decode did not read every token it was given.
#[derive(Debug)]
pub(crate) enum DecodeStop {
    /// The context's abort was raised.
    Aborted,
    /// llama.cpp failed it, in its words.
    Failed(String),
}

/// A context over a model, one sequence of KV cache, freed when dropped.
pub(crate) struct Context {
    pub(super) raw: NonNull<RawContext>,
    model: Arc<Model>,
    /// The abort llama.cpp asks during a decode, kept alive here.
    abort: Option<Abort>,
}

#[expect(
    unsafe_code,
    reason = "llama.h lets a context move between threads; every call here takes it by &mut"
)]
unsafe impl Send for Context {}

/// llama.cpp's question during a decode: whether the abort is raised.
#[expect(
    unsafe_code,
    reason = "llama.cpp hands back the flag Context::watch gave it"
)]
extern "C" fn abort_requested(data: *mut c_void) -> bool {
    // SAFETY: data is the flag of the Abort the context holds a clone of,
    // alive whenever the context decodes.
    unsafe { &*data.cast::<AtomicBool>() }.load(Ordering::Acquire)
}

impl Context {
    /// A context of `n_ctx` tokens over `model`, reading `n_batch` tokens a
    /// step, flash attention on, off or llama's choice (`None`), its KV
    /// cache in the types llama.cpp names `type_k` and `type_v`.
    #[expect(
        unsafe_code,
        reason = "the model is live and the names outlive the call"
    )]
    pub(crate) fn new(
        model: Arc<Model>,
        n_ctx: u32,
        n_batch: u32,
        flash_attn: Option<bool>,
        type_k: &str,
        type_v: &str,
    ) -> Result<Self, DecoderError> {
        let name = |t: &str| {
            CString::new(t)
                .map_err(|_| DecoderError::Load(format!("a cache type with a NUL: {t:?}")))
        };
        let (k, v) = (name(type_k)?, name(type_v)?);
        let flash_attn = flash_attn.map_or(-1, i32::from);
        // SAFETY: the model is live, and k and v are NUL-terminated.
        let raw = unsafe {
            gglib_llama_context_new(
                model.0.as_ptr(),
                n_ctx,
                n_batch,
                flash_attn,
                k.as_ptr(),
                v.as_ptr(),
            )
        };
        let raw = NonNull::new(raw)
            .ok_or_else(|| DecoderError::Load(last_error("llama.cpp could not make a context")))?;
        Ok(Self {
            raw,
            model,
            abort: None,
        })
    }

    pub(crate) fn model(&self) -> &Arc<Model> {
        &self.model
    }

    /// The context's size in tokens, as llama.cpp made it.
    #[expect(unsafe_code, reason = "reads a scalar from the live context")]
    pub(crate) fn n_ctx(&self) -> u32 {
        // SAFETY: the context is live.
        unsafe { gglib_llama_n_ctx(self.raw.as_ptr()) }
    }

    /// Have every decode from here on stop, inside llama.cpp, once `abort`
    /// is raised.
    #[expect(
        unsafe_code,
        reason = "the flag handed to llama.cpp lives in self.abort"
    )]
    pub(crate) fn watch(&mut self, abort: &Abort) {
        let flag = abort.as_ptr().cast_mut().cast::<c_void>();
        // SAFETY: the context is live, and the flag is kept alive by the
        // clone stored below until the next watch or the context's free.
        unsafe { gglib_llama_set_abort(self.raw.as_ptr(), Some(abort_requested), flag) };
        self.abort = Some(abort.clone());
    }

    /// `tokens` read into the KV cache after what it holds.
    #[expect(
        unsafe_code,
        reason = "the tokens outlive the call, which only reads them"
    )]
    pub(crate) fn decode(&mut self, tokens: &[Token]) -> Result<(), DecodeStop> {
        let n = i32::try_from(tokens.len())
            .map_err(|_| DecodeStop::Failed("more tokens than llama.cpp reads at once".into()))?;
        // SAFETY: the context is live, and tokens is n tokens.
        match unsafe { gglib_llama_decode(self.raw.as_ptr(), tokens.as_ptr(), n) } {
            0 => Ok(()),
            2 => Err(DecodeStop::Aborted),
            code => Err(DecodeStop::Failed(last_error(&format!(
                "llama_decode returned {code}"
            )))),
        }
    }

    /// The KV cache cut to its first `n` positions: false when it cannot be
    /// cut there. 0 empties it, which never fails.
    #[expect(unsafe_code, reason = "changes the live context's own KV cache")]
    pub(crate) fn keep(&mut self, n: u32) -> bool {
        let n = i32::try_from(n).unwrap_or(i32::MAX);
        // SAFETY: the context is live.
        unsafe { gglib_llama_memory_keep(self.raw.as_ptr(), n) }
    }

    /// The first position the KV cache holds, -1 when it is empty.
    #[expect(unsafe_code, reason = "reads a scalar from the live context")]
    pub(crate) fn pos_min(&self) -> i32 {
        // SAFETY: the context is live.
        unsafe { gglib_llama_memory_pos_min(self.raw.as_ptr()) }
    }
}

impl Drop for Context {
    #[expect(unsafe_code, reason = "frees the context this handle owns, once")]
    fn drop(&mut self) {
        // SAFETY: the context is live and freed here only, before its model
        // and its abort flag are dropped with the fields.
        unsafe { gglib_llama_context_free(self.raw.as_ptr()) }
    }
}
