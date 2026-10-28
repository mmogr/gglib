//! A model the shim loaded, and the progress its load reports.

use std::any::Any;
use std::ffi::{CStr, CString, c_void};
use std::os::unix::ffi::OsStrExt;
use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};
use std::path::Path;
use std::ptr::NonNull;

use super::{
    RawModel, gglib_llama_model_chat_template, gglib_llama_model_free, gglib_llama_model_load,
    gglib_llama_model_n_ctx_train, gglib_llama_model_n_swa, gglib_llama_token_to_piece,
    gglib_llama_tokenize, gglib_llama_vocab_add_bos, gglib_llama_vocab_add_eos,
    gglib_llama_vocab_bos, gglib_llama_vocab_eos, gglib_llama_vocab_is_eog, last_error,
};
use crate::decoder::{Abort, DecoderError, Token};

/// What a model's vocabulary and training say.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ModelFacts {
    pub(crate) n_ctx_train: i32,
    /// The sliding window of its attention, 0 when it has none.
    pub(crate) n_swa: i32,
    /// The BOS and EOS tokens, -1 when the vocabulary has none.
    pub(crate) bos: Token,
    pub(crate) eos: Token,
    pub(crate) add_bos: bool,
    pub(crate) add_eos: bool,
}

/// A model llama.cpp loaded, freed when dropped.
#[derive(Debug)]
pub(crate) struct Model(pub(super) NonNull<RawModel>);

#[expect(
    unsafe_code,
    reason = "a loaded llama_model is not written after its load, and llama.h lets any thread read it"
)]
unsafe impl Send for Model {}
#[expect(
    unsafe_code,
    reason = "a loaded llama_model is not written after its load, and llama.h lets threads read it at once"
)]
unsafe impl Sync for Model {}

/// What a load reports its progress to and checks, while llama.cpp reads.
struct Loading<'a> {
    progress: &'a mut dyn FnMut(f32),
    abort: &'a Abort,
    /// A panic of `progress`, carried past the C frames and resumed after.
    panic: Option<Box<dyn Any + Send>>,
}

/// llama.cpp's load progress: reported, and answered with whether to go on.
#[expect(
    unsafe_code,
    reason = "llama.cpp hands back the Loading that Model::load passed it"
)]
extern "C" fn on_progress(fraction: f32, data: *mut c_void) -> bool {
    // SAFETY: data is the `&mut Loading` Model::load passed for this load,
    // called back on the load's own thread before the load returns.
    let loading = unsafe { &mut *data.cast::<Loading<'_>>() };
    if loading.panic.is_some() {
        return false;
    }
    match catch_unwind(AssertUnwindSafe(|| (loading.progress)(fraction))) {
        Ok(()) => !loading.abort.is_raised(),
        Err(panic) => {
            loading.panic = Some(panic);
            false
        }
    }
}

impl Model {
    /// The model at `path`, its weights memory-mapped and `n_gpu_layers`
    /// of its layers offloaded, reporting to `progress` and stopped by
    /// `abort` at its next report.
    #[expect(
        unsafe_code,
        reason = "the path and the Loading the progress callback reads outlive the call"
    )]
    pub(crate) fn load(
        path: &Path,
        n_gpu_layers: i32,
        abort: &Abort,
        progress: &mut dyn FnMut(f32),
    ) -> Result<Self, DecoderError> {
        let c_path = CString::new(path.as_os_str().as_bytes())
            .map_err(|_| DecoderError::Load(format!("{} holds a NUL", path.display())))?;
        let mut loading = Loading {
            progress,
            abort,
            panic: None,
        };
        let data = std::ptr::from_mut(&mut loading).cast::<c_void>();
        // SAFETY: c_path is NUL-terminated, and `loading` lives until after
        // the call returns, the only time on_progress reads it.
        let raw = unsafe {
            gglib_llama_model_load(c_path.as_ptr(), n_gpu_layers, true, on_progress, data)
        };
        if let Some(panic) = loading.panic {
            resume_unwind(panic);
        }
        match NonNull::new(raw) {
            Some(raw) => Ok(Self(raw)),
            None if abort.is_raised() => Err(DecoderError::LoadAborted),
            None => Err(DecoderError::Load(last_error(
                "llama.cpp could not load it",
            ))),
        }
    }

    #[expect(
        unsafe_code,
        reason = "reads scalars from the live model this handle owns"
    )]
    pub(crate) fn facts(&self) -> ModelFacts {
        let m = self.0.as_ptr();
        // SAFETY: m is the live model this handle owns.
        unsafe {
            ModelFacts {
                n_ctx_train: gglib_llama_model_n_ctx_train(m),
                n_swa: gglib_llama_model_n_swa(m),
                bos: gglib_llama_vocab_bos(m),
                eos: gglib_llama_vocab_eos(m),
                add_bos: gglib_llama_vocab_add_bos(m),
                add_eos: gglib_llama_vocab_add_eos(m),
            }
        }
    }

    /// Its `tokenizer.chat_template`, empty when it has none.
    #[expect(unsafe_code, reason = "copies a string the live model owns")]
    pub(crate) fn chat_template(&self) -> String {
        // SAFETY: the model is live; the string it returns is NUL-terminated
        // and lives as long as the model.
        unsafe {
            let template = gglib_llama_model_chat_template(self.0.as_ptr());
            if template.is_null() {
                return String::new();
            }
            CStr::from_ptr(template).to_string_lossy().into_owned()
        }
    }

    #[expect(unsafe_code, reason = "asks the live model about a token")]
    pub(crate) fn is_eog(&self, token: Token) -> bool {
        // SAFETY: the model is live; any token number is checked by llama.
        unsafe { gglib_llama_vocab_is_eog(self.0.as_ptr(), token) }
    }

    /// `text` as tokens, BOS and EOS added as the vocabulary adds them and
    /// special tokens read as themselves, as llama-server tokenizes a prompt.
    #[expect(unsafe_code, reason = "the shim writes at most the room it is given")]
    pub(crate) fn tokenize(&self, text: &str) -> Result<Vec<Token>, DecoderError> {
        let too_long = || DecoderError::Tokenize("the text is longer than llama.cpp reads".into());
        let len = i32::try_from(text.len()).map_err(|_| too_long())?;
        // At most a token a byte, and BOS and EOS; a count asked back is room.
        let mut tokens: Vec<Token> = vec![0; text.len() + 2];
        for _ in 0..2 {
            let room = i32::try_from(tokens.len()).map_err(|_| too_long())?;
            // SAFETY: the model is live, text is `len` bytes, and tokens has
            // room for `room` tokens.
            let n = unsafe {
                gglib_llama_tokenize(
                    self.0.as_ptr(),
                    text.as_ptr().cast(),
                    len,
                    tokens.as_mut_ptr(),
                    room,
                    true,
                    true,
                )
            };
            match usize::try_from(n) {
                Ok(n) => {
                    tokens.truncate(n);
                    return Ok(tokens);
                }
                Err(_) if n == i32::MIN => break,
                Err(_) => tokens.resize(n.unsigned_abs() as usize, 0),
            }
        }
        Err(too_long())
    }

    /// `token`'s bytes, special tokens shown when `special`.
    #[expect(unsafe_code, reason = "the shim writes at most the room it is given")]
    pub(crate) fn piece(&self, token: Token, special: bool) -> Vec<u8> {
        let mut buf = vec![0_u8; 32];
        for _ in 0..2 {
            let room = i32::try_from(buf.len()).unwrap_or(i32::MAX);
            // SAFETY: the model is live, and buf has `room` bytes to write.
            let n = unsafe {
                gglib_llama_token_to_piece(
                    self.0.as_ptr(),
                    token,
                    buf.as_mut_ptr().cast(),
                    room,
                    special,
                )
            };
            match usize::try_from(n) {
                Ok(n) => {
                    buf.truncate(n);
                    return buf;
                }
                Err(_) => buf.resize(n.unsigned_abs() as usize, 0),
            }
        }
        Vec::new()
    }
}

impl Drop for Model {
    #[expect(unsafe_code, reason = "frees the model this handle owns, once")]
    fn drop(&mut self) {
        // SAFETY: no context or sampler holds the model any more: each holds
        // an Arc of it, and this is the last.
        unsafe { gglib_llama_model_free(self.0.as_ptr()) }
    }
}
