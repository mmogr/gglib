#![doc = include_str!("README.md")]

mod fit;

#[cfg(test)]
#[path = "admit_tests.rs"]
mod admit_tests;
#[cfg(test)]
#[path = "lifecycle_tests.rs"]
mod lifecycle_tests;
#[cfg(test)]
#[path = "load_tests.rs"]
mod load_tests;
#[cfg(test)]
mod test_support;

use std::fmt;
use std::io;
use std::sync::{Arc, Mutex, PoisonError};

use async_trait::async_trait;
use gglib_core::domain::device_budget::{DeviceReading, budget_from};
use gglib_core::ports::{
    Admission, LaunchOverrides, ModelCatalogPort, ModelLaunchSpec, ModelRuntimeError,
    ModelRuntimePort, RunningTarget,
};
use tokio::sync::watch;

use crate::decoder::{Abort, Decoder, LoadParams};
use crate::engine::Engine;
use crate::shim::{self, Shim};
use crate::state::EngineState;

/// gglib's model runtime on a device that cannot start llama-server: one
/// model at a time, loaded into this process and served by the shim.
///
/// It is the [`ModelRuntimePort`] gglib-proxy admits each chat through. An
/// admission looks the model up in the catalog, fits its context to the
/// device's budget, and loads it, freeing the model held before; the target
/// it hands the proxy is the shim, with the loaded context as its
/// `effective_ctx`. The budget is made once, from the [`DeviceReading`] the
/// runtime starts with, and held: every admission is fitted against the same
/// number, so the context a model gets does not move between requests.
///
/// [`InProcessRuntime::subscribe`] watches what the engine is doing.
/// [`InProcessRuntime::shutdown`] stops a load under way, frees the model,
/// stops the shim and ends that watch; a runtime shut down admits nothing.
pub struct InProcessRuntime {
    inner: Arc<Inner>,
    shim: Mutex<Option<Shim>>,
}

/// What an admission runs with, shared with the task that places a model.
struct Inner {
    engine: Arc<Engine>,
    catalog: Arc<dyn ModelCatalogPort>,
    budget_bytes: Option<u64>,
    /// The shim's port, the one every target names.
    port: u16,
    /// The model placed by admission. Held for the whole of a placement, so
    /// admissions and stops take turns.
    placed: tokio::sync::Mutex<Option<Placed>>,
    /// The target of the model held, readable while a placement runs.
    current: Mutex<Option<RunningTarget>>,
    /// Raised by shutdown; every load is given it, and stops when it is.
    closing: Abort,
}

/// A model loaded by admission.
struct Placed {
    id: u32,
    n_ctx: u32,
    target: RunningTarget,
}

impl InProcessRuntime {
    /// Start the shim for a new engine over `decoder` on a free port of
    /// 127.0.0.1, holding no model, its budget made from `reading`.
    pub async fn start(
        decoder: impl Decoder + 'static,
        catalog: Arc<dyn ModelCatalogPort>,
        reading: DeviceReading,
    ) -> io::Result<Self> {
        let engine = Engine::new(decoder);
        let shim = shim::start(Arc::clone(&engine)).await?;
        let inner = Inner {
            engine,
            catalog,
            budget_bytes: budget_from(&reading),
            port: shim.port(),
            placed: tokio::sync::Mutex::new(None),
            current: Mutex::new(None),
            closing: Abort::new(),
        };
        Ok(Self {
            inner: Arc::new(inner),
            shim: Mutex::new(Some(shim)),
        })
    }

    /// The budget every admission is fitted against, in bytes: `None` when
    /// the reading it was made from failed.
    pub fn budget_bytes(&self) -> Option<u64> {
        self.inner.budget_bytes
    }

    /// The engine's state now, then each change, until it is
    /// [`EngineState::Stopped`] and the watch ends.
    pub fn subscribe(&self) -> watch::Receiver<EngineState> {
        self.inner.engine.states().subscribe()
    }

    /// The engine's state now.
    pub fn state(&self) -> EngineState {
        self.inner.engine.states().current()
    }

    /// Stop a load under way and raise the abort of the reply in flight,
    /// then free the model once the decoder's lock is free, which the load
    /// and the reply's generation each hold until they return (the decode
    /// thread itself may still be sending its last event). Then stop the
    /// shim and end the watch on [`EngineState::Stopped`]. Idempotent.
    pub async fn shutdown(&self) -> io::Result<()> {
        let inner = &self.inner;
        inner.closing.raise();
        inner.engine.abort();
        let mut placed = inner.placed.lock().await;
        *placed = None;
        inner.forget();
        inner.unload().await;
        drop(placed);
        let shim = self
            .shim
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
        let stopped = match shim {
            Some(shim) => shim.shutdown().await,
            None => Ok(()),
        };
        inner.engine.states().stop();
        stopped
    }
}

impl Inner {
    /// Load `spec` with `n_ctx` unless it is held with that context already,
    /// in place of whatever is held.
    async fn place(
        self: Arc<Self>,
        spec: ModelLaunchSpec,
        n_ctx: u32,
    ) -> Result<Admission, ModelRuntimeError> {
        let mut placed = self.placed.lock().await;
        self.refuse_if_stopping()?;
        if let Some(held) = placed
            .as_ref()
            .filter(|p| p.id == spec.id && p.n_ctx == n_ctx)
        {
            return Ok(Admission::detached(held.target.clone()));
        }
        *placed = None;
        self.forget();
        let (k, v) = fit::CACHE_TYPES;
        let params = LoadParams {
            n_ctx,
            cache_type_k: k,
            cache_type_v: v,
        };
        let engine = Arc::clone(&self.engine);
        let (id, name, path) = (spec.id, spec.name.clone(), spec.file_path.clone());
        let closing = self.closing.clone();
        let loaded =
            tokio::task::spawn_blocking(move || engine.load(id, &name, &path, &params, &closing))
                .await
                .map_err(|e| ModelRuntimeError::Internal(format!("the load stopped: {e}")))?;
        // A shutdown during the load stopped it, or came as it ended.
        if let Err(stopped) = self.refuse_if_stopping() {
            self.unload().await;
            return Err(stopped);
        }
        let loaded = loaded.map_err(|e| ModelRuntimeError::SpawnFailed(e.to_string()))?;
        let target = RunningTarget::local(
            self.port,
            spec.id,
            spec.name,
            u64::from(loaded.n_ctx),
            false,
        )
        .with_model_sampling(spec.model_sampling)
        .with_slot_restore_supported(!spec.kv_memory_is_partial);
        *self.current.lock().unwrap_or_else(PoisonError::into_inner) = Some(target.clone());
        *placed = Some(Placed {
            id: spec.id,
            n_ctx,
            target: target.clone(),
        });
        drop(placed);
        let mut started = target;
        started.just_started = true;
        Ok(Admission::detached(started))
    }

    fn refuse_if_stopping(&self) -> Result<(), ModelRuntimeError> {
        if self.closing.is_raised() {
            return Err(ModelRuntimeError::Internal(
                "the in-process engine has been shut down".to_owned(),
            ));
        }
        Ok(())
    }

    /// Say no model is running.
    fn forget(&self) {
        *self.current.lock().unwrap_or_else(PoisonError::into_inner) = None;
    }

    /// Free the engine's model, from a blocking thread.
    async fn unload(&self) {
        let engine = Arc::clone(&self.engine);
        if let Err(e) = tokio::task::spawn_blocking(move || engine.unload()).await {
            tracing::warn!("unloading the in-process engine's model failed: {e}");
        }
    }

    /// Free the model placed, if `which` says it is the one to free.
    async fn stop(&self, which: impl FnOnce(&Placed) -> bool) -> bool {
        let mut placed = self.placed.lock().await;
        if !placed.as_ref().is_some_and(which) {
            return false;
        }
        *placed = None;
        self.forget();
        self.unload().await;
        // Held until the model is freed, so an admission waits for it.
        drop(placed);
        true
    }
}

#[async_trait]
impl ModelRuntimePort for InProcessRuntime {
    /// The model `model_name` names, loaded at the context that fits the
    /// budget, lowered to `num_ctx` when the request names a smaller one.
    /// `default_ctx` is not read: nobody on the device chose it, and as a
    /// cap it would hold every model at that size whatever fits. Nor are the
    /// overrides: the engine loads at the KV cache types its fit is made at,
    /// and takes none of llama-server's options.
    ///
    /// The admission is detached. A model swapped out ends the reply it was
    /// generating, as the engine's one slot serves one reply at a time.
    async fn admit(
        &self,
        model_name: &str,
        num_ctx: Option<u64>,
        _default_ctx: Option<u64>,
        _overrides: LaunchOverrides,
    ) -> Result<Admission, ModelRuntimeError> {
        let inner = &self.inner;
        inner.refuse_if_stopping()?;
        let spec = inner
            .catalog
            .resolve_for_launch(model_name)
            .await
            .map_err(|e| ModelRuntimeError::Internal(e.to_string()))?
            .ok_or_else(|| ModelRuntimeError::ModelNotFound(model_name.to_owned()))?;
        if spec.image_family.is_some() {
            return Err(ModelRuntimeError::ImageModelCannotChat(spec.name));
        }
        let n_ctx = fit::context_for(&spec, inner.budget_bytes, num_ctx).await?;
        // Placed on a task of its own, so a caller that stops waiting does
        // not leave a model loaded that the runtime does not know of.
        tokio::spawn(Arc::clone(inner).place(spec, n_ctx))
            .await
            .map_err(|e| ModelRuntimeError::Internal(format!("the admission stopped: {e}")))?
    }

    async fn current_model(&self) -> Option<RunningTarget> {
        self.inner
            .current
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    async fn stop_current(&self) -> Result<(), ModelRuntimeError> {
        self.inner.stop(|_| true).await;
        Ok(())
    }

    async fn stop_model(&self, model_id: u32) -> Result<bool, ModelRuntimeError> {
        Ok(self.inner.stop(|placed| placed.id == model_id).await)
    }
}

impl fmt::Debug for InProcessRuntime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("InProcessRuntime")
            .field("port", &self.inner.port)
            .field("budget_bytes", &self.inner.budget_bytes)
            .field("state", &self.state())
            .finish_non_exhaustive()
    }
}
