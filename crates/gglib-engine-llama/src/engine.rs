//! A decoder, the model it holds, and the one generation it runs at a time.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, RwLock};

use gglib_gguf::ChatTemplate;
use tokio::sync::Semaphore;

use crate::decoder::{
    Abort, Decoder, DecoderError, FinishReason, Finished, LoadParams, Loaded, Sampling, Step, Token,
};
use crate::state::{EngineState, States};

/// One decoder and the model it holds, shared by the shim's handlers and
/// whoever loads models into it.
///
/// [`Engine::load`] and [`Engine::unload`] block until a generation in
/// flight has ended, so a caller on an async runtime calls them from a
/// blocking thread, and raises [`Engine::abort`] first to end that
/// generation at its next step.
///
/// Each step it takes is an [`EngineState`]: a load is `Unloading` the model
/// held, `Loading` with each whole percent the decoder reports read, then
/// `Ready` or `Failed`, `Failed` too when the decoder panics, and `Idle` when
/// its abort stopped it; an unload is `Unloading` then `Idle`; a generation
/// is `Generating` then `Ready` again.
pub struct Engine {
    decoder: Mutex<Box<dyn Decoder>>,
    resident: RwLock<Option<Arc<Resident>>>,
    /// One permit: generations take turns, as one llama-server slot serves.
    pub(crate) turn: Arc<Semaphore>,
    /// The abort of the generation in flight.
    current: Mutex<Option<Abort>>,
    loads: AtomicU64,
    states: States,
}

/// The model the engine holds, as the shim serves it.
#[derive(Debug)]
pub(crate) struct Resident {
    /// Which load this is: a generation rendered for one load is not run
    /// on the next.
    pub(crate) id: u64,
    /// The name the model is served under.
    pub(crate) name: String,
    /// Where its GGUF is.
    pub(crate) path: PathBuf,
    /// What the decoder loaded it as.
    pub(crate) loaded: Loaded,
    /// Its chat template, compiled.
    pub(crate) template: ChatTemplate,
}

/// Why a generation did not run to its end.
#[derive(Debug)]
pub(crate) enum RunError {
    /// The model it was rendered for is no longer the one loaded.
    ModelChanged,
    /// The prompt does not leave a token free in the context.
    OverContext {
        n_prompt: u32,
        n_ctx: u32,
    },
    Decoder(DecoderError),
}

/// What a generation hands out as it runs.
#[derive(Debug)]
pub(crate) enum Progress {
    /// The prompt was tokenized and fits: generation begins.
    Started {
        prompt_tokens: u32,
    },
    Step(Step),
}

impl std::fmt::Debug for Engine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Engine")
            .field("resident", &self.resident())
            .finish_non_exhaustive()
    }
}

impl Engine {
    /// An engine over `decoder`, holding no model.
    pub fn new(decoder: impl Decoder + 'static) -> Arc<Self> {
        Arc::new(Self {
            decoder: Mutex::new(Box::new(decoder)),
            resident: RwLock::new(None),
            turn: Arc::new(Semaphore::new(1)),
            current: Mutex::new(None),
            loads: AtomicU64::new(0),
            states: States::new(),
        })
    }

    /// Load the model at `path`, served as `name`, in place of any model
    /// held: the generation in flight is aborted and waited for, and the
    /// model held is freed before the new one is read.
    ///
    /// Raising `abort` stops the load at the decoder's next report of its
    /// progress, leaving no model held, and the engine `Idle`. A chat
    /// template that does not compile fails the load and leaves no model
    /// held, as llama-server refuses to start on one. A load that panics is
    /// `Failed` as it unwinds, and the next load frees whatever the decoder
    /// was left holding.
    pub fn load(
        &self,
        name: &str,
        path: &Path,
        params: &LoadParams,
        abort: &Abort,
    ) -> Result<Loaded, DecoderError> {
        let _panicked = self.states.failed_on_panic(name);
        let failed = |e: DecoderError| {
            self.states.set(match e {
                DecoderError::LoadAborted => EngineState::Idle,
                _ => EngineState::Failed {
                    model: name.to_owned(),
                    reason: e.to_string(),
                },
            });
            e
        };
        let mut decoder = self.free();
        self.states.loading(name, 0.0);
        let loaded = decoder
            .load(path, params, abort, &mut |read| {
                self.states.loading(name, read);
            })
            .map_err(&failed)?;
        let template = match ChatTemplate::new(&loaded.chat_template, loaded.vocab.clone()) {
            Ok(template) => template,
            Err(e) => {
                decoder.unload();
                return Err(failed(DecoderError::Load(e.to_string())));
            }
        };
        *write(&self.resident) = Some(Arc::new(Resident {
            id: self.loads.fetch_add(1, Ordering::Relaxed),
            name: name.to_owned(),
            path: path.to_owned(),
            loaded: loaded.clone(),
            template,
        }));
        self.states.set(EngineState::Ready {
            model: name.to_owned(),
            n_ctx: loaded.n_ctx,
        });
        // Held until here, so a load and an unload leave the model held, the
        // model served and the state in step.
        drop(decoder);
        Ok(loaded)
    }

    /// Free the model held, after aborting and waiting for the generation
    /// in flight.
    pub fn unload(&self) {
        let decoder = self.free();
        self.states.set(EngineState::Idle);
        drop(decoder);
    }

    /// Stop serving the model held, end the generation in flight and wait
    /// for it, and free the model: the decoder, its lock held, holding
    /// nothing.
    fn free(&self) -> MutexGuard<'_, Box<dyn Decoder>> {
        if let Some(held) = self.evict() {
            self.states.set(EngineState::Unloading {
                model: held.name.clone(),
            });
        }
        let mut decoder = lock(&self.decoder);
        // A load that had the lock before this call took it may have made a
        // model resident after the eviction above.
        let late = write(&self.resident).take();
        if let Some(held) = late {
            self.states.set(EngineState::Unloading {
                model: held.name.clone(),
            });
        }
        decoder.unload();
        decoder
    }

    /// What the model held was loaded as, if one is.
    pub fn loaded(&self) -> Option<Loaded> {
        self.resident().map(|r| r.loaded.clone())
    }

    /// End the generation in flight, if there is one, at its next step.
    pub fn abort(&self) {
        if let Some(abort) = lock(&self.current).as_ref() {
            abort.raise();
        }
    }

    /// Serve no model from here on, and end the generation in flight. The
    /// model that was served, if one was.
    ///
    /// Under the same lock [`Engine::run`] checks the model and records its
    /// abort under, so a generation either sees no model and does not start,
    /// or started and is aborted here.
    fn evict(&self) -> Option<Arc<Resident>> {
        let current = lock(&self.current);
        let held = write(&self.resident).take();
        if let Some(abort) = current.as_ref() {
            abort.raise();
        }
        drop(current);
        held
    }

    /// Where the engine's [`EngineState`] is published.
    pub(crate) fn states(&self) -> &States {
        &self.states
    }

    pub(crate) fn resident(&self) -> Option<Arc<Resident>> {
        self.resident
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Tokenize `prompt` and generate from it on the model loaded as
    /// `resident`, handing out its [`Progress`]. Blocks; the caller holds a
    /// [`Engine::turn`] permit.
    ///
    /// A prompt of `n_ctx` tokens or more is refused before any is read,
    /// as llama-server refuses one. An empty prompt is answered as
    /// llama-server answers it ("empty prompt - releasing slot"): a reply of
    /// nothing, ended at its length, which the decoder is not asked for.
    ///
    /// The model is `Generating` while it runs. Both changes are made under
    /// the decoder's lock, which a load or an unload waits for, so neither
    /// lands on a model loaded after this one.
    pub(crate) fn run(
        &self,
        resident: u64,
        prompt: &str,
        sampling: &Sampling,
        abort: &Abort,
        on: &mut dyn FnMut(Progress),
    ) -> Result<Finished, RunError> {
        let mut decoder = lock(&self.decoder);
        let held = {
            let mut current = lock(&self.current);
            let Some(held) = self.resident().filter(|r| r.id == resident) else {
                return Err(RunError::ModelChanged);
            };
            *current = Some(abort.clone());
            held
        };
        let n_ctx = held.loaded.n_ctx;
        self.states.generating(&held.name);
        let result = generate(&mut **decoder, n_ctx, prompt, sampling, abort, on);
        self.states.generated(&held.name, n_ctx);
        drop(decoder);
        *lock(&self.current) = None;
        result
    }
}

/// Tokenize `prompt`, refuse it if it leaves no token of `n_ctx` free, and
/// generate from it unless it is empty.
fn generate(
    decoder: &mut dyn Decoder,
    n_ctx: u32,
    prompt: &str,
    sampling: &Sampling,
    abort: &Abort,
    on: &mut dyn FnMut(Progress),
) -> Result<Finished, RunError> {
    let tokens: Vec<Token> = decoder.tokenize(prompt).map_err(RunError::Decoder)?;
    let n_prompt = u32::try_from(tokens.len()).unwrap_or(u32::MAX);
    if n_prompt >= n_ctx {
        return Err(RunError::OverContext { n_prompt, n_ctx });
    }
    on(Progress::Started {
        prompt_tokens: n_prompt,
    });
    if tokens.is_empty() {
        return Ok(Finished {
            reason: FinishReason::Length,
            cached_tokens: 0,
            completion_tokens: 0,
        });
    }
    decoder
        .generate(&tokens, sampling, abort, &mut |step| {
            on(Progress::Step(step));
        })
        .map_err(RunError::Decoder)
}

fn lock<T: ?Sized>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

fn write<T>(l: &RwLock<T>) -> std::sync::RwLockWriteGuard<'_, T> {
    l.write().unwrap_or_else(PoisonError::into_inner)
}
