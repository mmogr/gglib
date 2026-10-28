#![doc = include_str!("README.md")]

mod context;
mod model;
mod sampler;

use std::ffi::{CStr, c_char, c_void};
use std::marker::{PhantomData, PhantomPinned};
use std::sync::{Mutex, PoisonError};

pub(crate) use context::{Context, DecodeStop};
pub(crate) use model::Model;
pub(crate) use sampler::Sampler;

/// The shim's opaque handles, only ever pointed at.
#[repr(C)]
struct RawModel {
    _opaque: [u8; 0],
    _marker: PhantomData<(*mut u8, PhantomPinned)>,
}
#[repr(C)]
struct RawContext {
    _opaque: [u8; 0],
    _marker: PhantomData<(*mut u8, PhantomPinned)>,
}
#[repr(C)]
struct RawSampler {
    _opaque: [u8; 0],
    _marker: PhantomData<(*mut u8, PhantomPinned)>,
}

type ProgressFn = extern "C" fn(fraction: f32, data: *mut c_void) -> bool;
type AbortFn = extern "C" fn(data: *mut c_void) -> bool;

#[expect(
    unsafe_code,
    reason = "the C shim, declared as csrc/gglib_llama.h declares it; the calls with no pointer are safe"
)]
unsafe extern "C" {
    safe fn gglib_llama_backend_init();
    safe fn gglib_llama_backend_free();
    safe fn gglib_llama_supports_gpu_offload() -> bool;
    #[cfg(test)]
    safe fn gglib_llama_live_objects() -> i32;
    safe fn gglib_llama_last_error() -> *const c_char;
    fn gglib_llama_model_load(
        path: *const c_char,
        n_gpu_layers: i32,
        use_mmap: bool,
        progress: ProgressFn,
        data: *mut c_void,
    ) -> *mut RawModel;
    fn gglib_llama_model_free(model: *mut RawModel);
    fn gglib_llama_model_n_ctx_train(model: *const RawModel) -> i32;
    fn gglib_llama_model_n_swa(model: *const RawModel) -> i32;
    fn gglib_llama_model_chat_template(model: *const RawModel) -> *const c_char;
    fn gglib_llama_vocab_bos(model: *const RawModel) -> i32;
    fn gglib_llama_vocab_eos(model: *const RawModel) -> i32;
    fn gglib_llama_vocab_add_bos(model: *const RawModel) -> bool;
    fn gglib_llama_vocab_add_eos(model: *const RawModel) -> bool;
    fn gglib_llama_vocab_is_eog(model: *const RawModel, token: i32) -> bool;
    fn gglib_llama_tokenize(
        model: *const RawModel,
        text: *const c_char,
        text_len: i32,
        tokens: *mut i32,
        n_max: i32,
        add_special: bool,
        parse_special: bool,
    ) -> i32;
    fn gglib_llama_token_to_piece(
        model: *const RawModel,
        token: i32,
        buf: *mut c_char,
        len: i32,
        special: bool,
    ) -> i32;
    fn gglib_llama_context_new(
        model: *mut RawModel,
        n_ctx: u32,
        n_batch: u32,
        flash_attn: i32,
        type_k: *const c_char,
        type_v: *const c_char,
    ) -> *mut RawContext;
    fn gglib_llama_context_free(ctx: *mut RawContext);
    fn gglib_llama_n_ctx(ctx: *const RawContext) -> u32;
    fn gglib_llama_set_abort(ctx: *mut RawContext, abort: Option<AbortFn>, data: *mut c_void);
    fn gglib_llama_decode(ctx: *mut RawContext, tokens: *const i32, n_tokens: i32) -> i32;
    fn gglib_llama_memory_keep(ctx: *mut RawContext, n: i32) -> bool;
    fn gglib_llama_memory_pos_min(ctx: *const RawContext) -> i32;
    fn gglib_llama_sampler_new(
        model: *const RawModel,
        temperature: f32,
        top_k: i32,
        top_p: f32,
        min_p: f32,
        typical_p: f32,
        top_n_sigma: f32,
        dynatemp_range: f32,
        dynatemp_exponent: f32,
        repeat_penalty: f32,
        repeat_last_n: i32,
        presence_penalty: f32,
        frequency_penalty: f32,
        dry_multiplier: f32,
        dry_base: f32,
        dry_allowed_length: i32,
        dry_penalty_last_n: i32,
        xtc_probability: f32,
        xtc_threshold: f32,
        mirostat: i32,
        mirostat_tau: f32,
        mirostat_eta: f32,
        min_keep: i32,
        seed: u32,
        ignore_eos: bool,
    ) -> *mut RawSampler;
    fn gglib_llama_sampler_accept(sampler: *mut RawSampler, token: i32);
    fn gglib_llama_sampler_sample(sampler: *mut RawSampler, ctx: *mut RawContext) -> i32;
    fn gglib_llama_sampler_free(sampler: *mut RawSampler);
}

/// What llama.cpp logged as errors on this thread during the shim's last
/// call that can fail, or `fallback` when it logged none.
#[expect(
    unsafe_code,
    reason = "reads the shim's thread-local error text, NUL-terminated and alive as long as the thread"
)]
fn last_error(fallback: &str) -> String {
    // SAFETY: the shim returns its thread-local buffer, always NUL-terminated.
    let logged = unsafe { CStr::from_ptr(gglib_llama_last_error()) }.to_string_lossy();
    match logged.trim() {
        "" => fallback.to_owned(),
        text => text.to_owned(),
    }
}

/// How many [`Backend`]s exist.
static BACKEND_USERS: Mutex<usize> = Mutex::new(0);

/// llama's backends, initialised while any `Backend` exists, with its log
/// going through the shim.
#[derive(Debug)]
pub(crate) struct Backend(());

impl Backend {
    pub(crate) fn acquire() -> Self {
        let mut users = BACKEND_USERS.lock().unwrap_or_else(PoisonError::into_inner);
        if *users == 0 {
            gglib_llama_backend_init();
        }
        *users += 1;
        drop(users);
        Self(())
    }
}

impl Drop for Backend {
    fn drop(&mut self) {
        let mut users = BACKEND_USERS.lock().unwrap_or_else(PoisonError::into_inner);
        *users -= 1;
        if *users == 0 {
            gglib_llama_backend_free();
        }
    }
}

/// Whether this build of llama.cpp can offload layers to a GPU.
pub(crate) fn supports_gpu_offload() -> bool {
    gglib_llama_supports_gpu_offload()
}

/// The models, contexts and samplers the shim made and has not freed.
#[cfg(test)]
pub(crate) fn live_objects() -> i32 {
    gglib_llama_live_objects()
}
