#![doc = include_str!("README.md")]
mod launch;
mod spawned_child;
mod vram;

use std::sync::Arc;
use std::time::Duration;

use gglib_core::domain::SecondarySlotDecision;
use gglib_core::ports::{
    Admission, CatalogError, LaunchOverrides, ModelCatalogPort, ModelLaunchSpec, ModelRuntimeError,
    PinnedSpec, RunningTarget,
};
use gglib_core::server_config::{
    CacheRamSetting, ContextSizeSource, ServerConfigOptions, resolve_context_size_with_source,
};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use crate::process::admission::{
    AdmissionDecision, AdmissionQueue, PRIMARY_SLOT, Resident, Ticket, launch_timeout,
};
use crate::process::core::GuiProcessCore;
use crate::process::health::check_http_health;
use launch::{LaunchRequest, run as run_launch};

pub use vram::ram_available_for;

/// How often a waiting request re-evaluates its position while nothing wakes it.
///
/// A turn ageing past its quantum is a purely temporal transition: nothing
/// happens to fire an event, the clock simply passes. This is what makes it
/// observable. Short enough to be imperceptible next to a model swap, long
/// enough that a hundred queued requests are not a busy loop.
const POLL_TICK: Duration = Duration::from_millis(250);

/// The models resident in VRAM, and the machinery that puts them there.
///
/// Holds the standing launch configuration, the pin, and the
/// [`AdmissionQueue`] that decides who gets a slot. See the
/// [module docs](self) for how one admission is shaped.
pub struct ResidentSet {
    /// Model catalog for resolving model names into launch specifications.
    catalog: Arc<dyn ModelCatalogPort>,
    /// Who is resident, who is waiting, and whose turn it is.
    queue: Arc<AdmissionQueue>,
    /// Standing launch options every spawn starts from. See the
    /// [module docs](self) for how it composes with per-call overrides.
    launch_overrides: ServerConfigOptions,
    /// How to size llama-server's host-RAM prompt cache (`--cache-ram`).
    ///
    /// Not part of [`Self::launch_overrides`] because it is not a flag to pass
    /// through: it is resolved at spawn against live system RAM, the model's
    /// weights and its KV footprint at the launch context size.
    cache_ram: CacheRamSetting,
    /// The one model this set will serve, if pinned.
    ///
    /// `Some(spec)` makes every other model a hard error instead of a swap —
    /// see [`Self::check_pinned`] — and layers the spec's launch overrides onto
    /// [`Self::launch_overrides`] at launch. `None` is the ordinary
    /// auto-swapping proxy.
    ///
    /// Mutable at runtime (behind a lock) rather than fixed at construction:
    /// the daemon owns one long-lived manager, and `gglib serve` pins it over
    /// HTTP for the lifetime of one proxy run.
    pinned: std::sync::RwLock<Option<PinnedSpec>>,
}

/// Resolve one launch's [`ServerConfigOptions`] and context size together.
///
/// Pulled out of the admission path so the context-resolution logic can be
/// tested without spawning a process — this is the exact computation that was
/// previously duplicated as a wrong `effective_ctx`
/// (`num_ctx.unwrap_or(default_ctx)`, skipping `model_server_ctx` entirely) and
/// a separate, correct `resolve_context_size` call used only for the KV-cache
/// estimate. See #685.
///
/// The context chain is assigned onto the overlaid template rather than
/// overlaid itself: the manager is authoritative for every rung, and a
/// stale `model_server_ctx` inherited from `template` would silently size the
/// launch for a different model than `model_server_ctx` names here.
fn resolve_launch_opts(
    template: &ServerConfigOptions,
    per_call: &ServerConfigOptions,
    num_ctx: Option<u64>,
    default_ctx: Option<u64>,
    fitted_ctx: Option<u64>,
    model_server_ctx: Option<usize>,
) -> (ServerConfigOptions, u64, ContextSizeSource) {
    let mut opts = template.overlay(per_call);
    opts.context_size = num_ctx.or(opts.context_size);
    opts.model_server_ctx = model_server_ctx;
    opts.fitted_ctx = fitted_ctx;
    // Assigned as given, not `Some(default_ctx)`: a user who set nothing must
    // fall through to the fitted rung rather than be handed the floor as
    // though they had chosen it.
    opts.global_default_ctx = default_ctx;
    let (resolved_ctx, ctx_source) = resolve_context_size_with_source(&opts);
    (opts, resolved_ctx, ctx_source)
}

/// Fit against the reserved budget, falling back to the undivided device.
///
/// A seam, not indirection: the chain is the whole of the co-resident
/// reservation's escape hatch, and inlining it left the behaviour unguarded —
/// deleting the fallback passed every test in the crate.
///
/// The seam guards the logic, not the wiring. Passing the same budget as both
/// arguments still neuters the fallback and no test would notice; catching that
/// needs `admit` exercised end to end against a catalog, which this module does
/// not do.
fn fit_or_undivided<F>(fit: F, reserved: Option<u64>, undivided: Option<u64>) -> Option<u64>
where
    F: Fn(Option<u64>) -> Option<u64>,
{
    fit(reserved).or_else(|| fit(undivided))
}

/// Removes a ticket from the queue on every exit path.
///
/// The one that matters is the path with no code on it: an abandoned request —
/// a client that hung up, a future that was dropped — must stop holding the
/// front of the queue, or it goes on forcing swaps that nobody is waiting for.
struct QueuedTicket {
    queue: Arc<AdmissionQueue>,
    ticket: Ticket,
}

impl Drop for QueuedTicket {
    fn drop(&mut self) {
        self.queue.abandon(&self.ticket);
    }
}

impl ResidentSet {
    /// Create an auto-swapping resident set with a standing options template.
    pub(super) fn new(
        catalog: Arc<dyn ModelCatalogPort>,
        launch_overrides: ServerConfigOptions,
        cache_ram: CacheRamSetting,
    ) -> Self {
        Self {
            catalog,
            queue: Arc::new(AdmissionQueue::new()),
            launch_overrides,
            cache_ram,
            pinned: std::sync::RwLock::new(None),
        }
    }

    /// The admission queue, for callers that only need to read or evict.
    pub(super) fn queue(&self) -> &Arc<AdmissionQueue> {
        &self.queue
    }

    /// The model this set is pinned to, if any.
    ///
    /// The read side of [`Self::check_pinned`]: callers that want to avoid
    /// provoking a mismatch rather than handle one need to know the name up
    /// front.
    pub(super) fn pinned_name(&self) -> Option<String> {
        self.pinned
            .read()
            .ok()
            .and_then(|guard| guard.as_ref().map(|p| p.name.clone()))
    }

    /// Pin this set to one model, or clear the pin.
    pub(super) fn set_pin(&self, pin: Option<PinnedSpec>) {
        if let Ok(mut guard) = self.pinned.write() {
            *guard = pin;
        }
    }

    /// Reject a request for any model other than the pinned one.
    ///
    /// Checked before the queue is consulted, so a foreign request fails
    /// immediately rather than queueing behind — or worse, displacing — the
    /// pinned model.
    pub(super) fn check_pinned(&self, model_name: &str) -> Result<(), ModelRuntimeError> {
        match self.pinned_name() {
            Some(expected) if expected != model_name => {
                Err(ModelRuntimeError::PinnedModelMismatch {
                    expected,
                    requested: model_name.to_owned(),
                })
            }
            _ => Ok(()),
        }
    }

    /// Admit a request to a running model, launching or swapping if needed.
    ///
    /// See the [module docs](self) for why everything model-static is resolved
    /// before the request joins the queue.
    pub(super) async fn admit(
        &self,
        core: &Arc<RwLock<GuiProcessCore>>,
        model_name: &str,
        num_ctx: Option<u64>,
        default_ctx: Option<u64>,
        overrides: LaunchOverrides,
    ) -> Result<Admission, ModelRuntimeError> {
        // Refuse foreign models before touching the queue, so a rejected
        // request neither queues behind the pinned model nor displaces it.
        self.check_pinned(model_name)?;

        let spec = self.resolve(model_name).await?;
        let spec_weights_bytes = spec.file_size_bytes;

        // A pin's launch overrides layer onto the standing template, winning
        // field-wise: they are the *output* of the caller's full cascade
        // (`UnifiedServerConfig::resolved_options`), so letting the run-wide
        // template win would undo a cascade that has already run.
        let template = match self.pinned.read().ok().and_then(|g| g.clone()) {
            Some(pin) => self.launch_overrides.overlay(&pin.launch_overrides),
            None => self.launch_overrides.clone(),
        };
        let cache_ram = overrides.cache_ram.unwrap_or(self.cache_ram);

        // What this machine could serve for this model, if nobody has said
        // otherwise. Ranks below an explicit request, a per-model default and
        // a global setting the user actually chose — and above the built-in
        // floor, which is what you serve when you know nothing.
        //
        // Derived from the model's own GGUF facts and the device's nominal
        // capacity less a fixed reservation for the second slot. `None` when
        // any of that is unknown, which falls through to the floor rather than
        // guessing.
        //
        // KV types come from the *overlaid* options, in the same precedence the
        // launch will use (`over` wins). Reading the template first would size
        // the fit against one KV type and launch with another — and a
        // q8_0/f16 disagreement is a factor of two on the cache being
        // budgeted.
        let overlaid = template.overlay(&overrides.options);
        let kv_types = crate::llama::args::resolve_kv_cache_types(
            overlaid.cache_type_k,
            overlaid.cache_type_v,
        );
        // One closure, two budgets. Spelling the six-argument call out twice is
        // how the fit and the launch once came to disagree about which KV cache
        // type they were sizing for.
        let fit_against = |budget| {
            let (fitted, inputs) = gglib_core::domain::fit_context_explained(
                spec.context_length,
                Some(spec.file_size_bytes),
                spec.kv_elems_per_token,
                kv_types.k,
                kv_types.v,
                budget,
            );
            // The two constants behind this are judgement calls, and the only
            // way they stop being guesses is if the numbers they produced are
            // visible when somebody looks. Nothing reads this.
            debug!(
                model = %spec.name,
                budget_bytes = ?inputs.budget_bytes,
                weights_bytes = ?inputs.weights_bytes,
                kv_bytes_per_token = ?inputs.kv_bytes_per_token,
                trained_ctx = ?inputs.trained_ctx,
                unsnapped = ?inputs.unsnapped,
                fitted = ?fitted,
                "context fit"
            );
            fitted
        };

        // `enabled`, not "is the variable present": every other switch reads
        // truthiness through this helper, and `active()` — what the daemon
        // reports as in effect — filters by the same call. Reading presence
        // would turn the fit off for `=0` while the roster said it was
        // untouched, which is the confident-wrong-conclusion failure
        // `debug_switches` exists to prevent.
        //
        // Reserving room for a co-resident is worth a rung, not the whole
        // feature. On a small card a large model can fail to fit at all once
        // the reservation is taken — and that is exactly the machine where a
        // full-ceiling secondary could never have loaded anyway, so the
        // reservation buys nothing and costs everything. Fall back to the
        // undivided device.
        //
        // The two budgets are one closure apart on purpose: they differ only
        // in the budget, and two spelled-out copies of the same six-argument
        // call is how the fit and the launch once came to disagree about which
        // KV cache type they were sizing for.
        //
        // Known consequence, deliberate: this makes the served context
        // non-monotonic in device size. A 6.5 GiB card cannot host a secondary,
        // so its primary takes the whole device; a 7 GiB card can, so it yields
        // room for one and its primary gets less. Upgrading that card lowers
        // the context served for the same model.
        let fitted_ctx = (!gglib_core::debug_switches::enabled("GGLIB_DISABLE_CONTEXT_FIT"))
            .then(|| {
                fit_or_undivided(
                    fit_against,
                    vram::fit_budget_for(),
                    crate::system::total_device_memory_bytes(),
                )
            })
            .flatten();

        // Resolved once, here, before anything else runs. This is the single
        // source of truth for "what context is this launch/reuse decision
        // about" — read by the resident-match test below, every log line, the
        // tracked `Resident`, and the `RunningTarget` returned to the proxy.
        let (opts, resolved_ctx, ctx_source) = resolve_launch_opts(
            &template,
            &overrides.options,
            num_ctx,
            default_ctx,
            fitted_ctx,
            spec.server_defaults
                .as_ref()
                .and_then(|sc| sc.context_length),
        );

        self.wait_for_slot(
            core,
            LaunchRequest {
                spec,
                opts,
                context: (resolved_ctx, ctx_source),
                slot: PRIMARY_SLOT, // replaced by the queue's decision
                evict: None,
                cache_ram,
                health_deadline_secs: crate::process::health::launch_deadline_secs(
                    spec_weights_bytes,
                ),
            },
        )
        .await
    }

    /// Resolve a model name to its launch specification.
    ///
    /// Ahead of the queue on purpose: a model nobody has should 404 straight
    /// away rather than after waiting out a swap to discover it.
    async fn resolve(&self, model_name: &str) -> Result<ModelLaunchSpec, ModelRuntimeError> {
        self.catalog
            .resolve_for_launch(model_name)
            .await
            .map_err(|e| match e {
                CatalogError::QueryFailed(msg) | CatalogError::Internal(msg) => {
                    ModelRuntimeError::Internal(msg)
                }
            })?
            .ok_or_else(|| ModelRuntimeError::ModelNotFound(model_name.to_owned()))
    }

    /// Queue for a slot, then act on whatever the scheduler decides.
    async fn wait_for_slot(
        &self,
        core: &Arc<RwLock<GuiProcessCore>>,
        request: LaunchRequest,
    ) -> Result<Admission, ModelRuntimeError> {
        let model_name = request.spec.name.clone();
        let resolved_ctx = request.context.0;
        let mut queued = QueuedTicket {
            queue: Arc::clone(&self.queue),
            ticket: self.queue.enqueue(&model_name),
        };

        loop {
            // Subscribed and enabled *before* the poll below, so a wakeup that
            // fires between deciding to wait and starting to wait is still
            // delivered rather than costing a full `POLL_TICK` for nothing.
            let changed = self.queue.subscribe();
            tokio::pin!(changed);
            changed.as_mut().enable();

            match self
                .queue
                .poll(&queued.ticket, self.secondary_verdict(&request))
            {
                AdmissionDecision::Serve { slot } => {
                    if let Some(admission) = self.serve(slot, resolved_ctx, core).await {
                        return Ok(admission);
                    }
                    // The resident turned out to be unusable and has been
                    // evicted; go round again and launch a fresh one — with a
                    // *fresh ticket*. `poll` forgot this one the moment it
                    // granted the serve, and a forgotten ticket is invisible
                    // to the scheduler: it can never be the oldest waiter, so
                    // it can never win a launch, and its only remaining exit
                    // was waiting out the deadline for a 503. A busy server
                    // failing its health check put real requests on exactly
                    // that path.
                    queued.ticket = self.queue.enqueue(&model_name);
                }
                AdmissionDecision::Launch { slot, evict } => {
                    return self.launch(core, &request, slot, evict).await;
                }
                AdmissionDecision::Wait => {
                    tokio::select! {
                        () = changed => {}
                        () = tokio::time::sleep(POLL_TICK) => {}
                    }
                }
                AdmissionDecision::Expired => {
                    warn!(
                        model = %model_name,
                        "admission queue made no progress for the whole deadline — surfacing 503"
                    );
                    return Err(ModelRuntimeError::AdmissionTimeout(format!(
                        "the queue made no progress while '{model_name}' waited — \
                         no launch running, no generation finishing"
                    )));
                }
            }
        }
    }

    /// Whether this request's model may co-reside in the second slot.
    ///
    /// Judged against a free-VRAM reading taken now rather than at enqueue
    /// time: the primary model may have finished loading since the request
    /// queued, and a decision made against the earlier figure would be about a
    /// machine that no longer exists. Re-computed on every poll tick, so the
    /// verdict the queue acts on is at most one [`POLL_TICK`] old — on top of
    /// the probe's own short-lived cache.
    ///
    /// Computed *before* [`AdmissionQueue::poll`] takes the queue's lock,
    /// necessarily: the probe can block (it may fork `nvidia-smi`), and no
    /// caller code may run inside the queue's critical section — a callback
    /// version of this once re-entered the queue from under its own lock and
    /// deadlocked the daemon (#721).
    fn secondary_verdict(&self, request: &LaunchRequest) -> SecondarySlotDecision {
        let kv_types = crate::llama::args::resolve_kv_cache_types(
            request.opts.cache_type_k,
            request.opts.cache_type_v,
        );
        let decision = vram::secondary_slot_decision(&request.spec, kv_types, request.context.0);
        if !decision.is_grant() {
            debug!(
                model = %request.spec.name,
                verdict = decision.label(),
                "second resident slot refused"
            );
        }
        decision
    }

    /// Serve from a model already resident in `slot`.
    ///
    /// Returns `None` when the resident cannot serve this request after all —
    /// it was launched with a different context size, or it has stopped
    /// answering its health check. Both cases evict it, so the caller's next
    /// pass launches a fresh instance.
    async fn serve(
        &self,
        slot: usize,
        resolved_ctx: u64,
        core: &Arc<RwLock<GuiProcessCore>>,
    ) -> Option<Admission> {
        // `poll` has already counted this request against the slot, so the
        // lease must be claimed here even on the reject paths — dropping it is
        // what balances the count.
        let lease = self.queue.claim(slot);
        let resident = self.queue.slot(slot)?;

        if resident.context_size != resolved_ctx {
            info!(
                model_name = %resident.model_name,
                running_context = %resident.context_size,
                requested_context = %resolved_ctx,
                "resident model was launched with a different context — recycling"
            );
            drop(lease);
            self.recycle(slot, core).await;
            return None;
        }

        if !check_http_health(resident.port).await {
            warn!(
                model_id = %resident.model_id,
                port = %resident.port,
                "resident model failed health check; recycling degraded instance"
            );
            drop(lease);
            self.recycle(slot, core).await;
            return None;
        }

        debug!(
            model_id = %resident.model_id,
            model_name = %resident.model_name,
            port = %resident.port,
            context = %resident.context_size,
            slot = %slot,
            "Model already resident"
        );

        let mut target = RunningTarget::local(
            resident.port,
            resident.model_id,
            resident.model_name.clone(),
            resident.context_size,
            false, // already resident — not a fresh spawn
        )
        .with_slot_restore_supported(resident.slot_restore_supported)
        .with_model_sampling(resident.model_sampling)
        .with_cache_ram_health(resident.cache_ram_health);

        // Reused verbatim rather than re-narrated: this instance was launched
        // with whatever the stored narration says, and re-resolving now against
        // current settings would describe a launch that never happened.
        if let Some(narration) = resident.narration.clone() {
            target = target.with_narration(narration);
        }

        Some(Admission { target, lease })
    }

    /// Drive one launch, detached from this request's future.
    ///
    /// `tokio::spawn` rather than an inline `await` so a client that
    /// disconnects mid-launch cannot abort a startup other requests are waiting
    /// on. Dropping the returned `JoinHandle` does not cancel the task, so the
    /// launch completes and records itself either way.
    async fn launch(
        &self,
        core: &Arc<RwLock<GuiProcessCore>>,
        request: &LaunchRequest,
        slot: usize,
        evict: Option<u32>,
    ) -> Result<Admission, ModelRuntimeError> {
        let core = Arc::clone(core);
        let queue = Arc::clone(&self.queue);
        // Along for one purpose: recording the fresh spawn's template-caps
        // observation on the model row once it is health-ready (ADR 0007).
        let catalog = Arc::clone(&self.catalog);
        let launch_request = LaunchRequest {
            spec: request.spec.clone(),
            opts: request.opts.clone(),
            context: request.context,
            slot,
            evict,
            // A co-resident model gets no prompt cache of its own; the primary
            // is where reuse actually pays.
            cache_ram: if slot == PRIMARY_SLOT {
                request.cache_ram
            } else {
                vram::secondary_cache_ram()
            },
            // Carried through, not re-derived: the budget below reads the same
            // field the launch waits on.
            health_deadline_secs: request.health_deadline_secs,
        };

        // Sized to this model, not to a constant, and read from the same field
        // the launch itself waits on: a flat budget here would cut that wait
        // short — dropping the future mid-await, which skips the cleanup that
        // stops the child and leaks it.
        let budget = launch_timeout(Duration::from_secs(launch_request.health_deadline_secs));

        let handle = tokio::spawn(async move {
            match tokio::time::timeout(
                budget,
                run_launch(core, Arc::clone(&queue), catalog, launch_request),
            )
            .await
            {
                Ok(result) => result,
                Err(_) => {
                    // The launch overran its budget. Free the slot explicitly:
                    // the future was dropped mid-flight, so nothing inside it
                    // will do so.
                    queue.launch_failed(slot);
                    Err(ModelRuntimeError::Internal(
                        "model startup exceeded its deadline".to_string(),
                    ))
                }
            }
        });

        match handle.await {
            Ok(Ok((target, lease))) => Ok(Admission { target, lease }),
            Ok(Err(e)) => Err(e),
            Err(join_error) => {
                // The task panicked, so its own cleanup never ran.
                self.queue.launch_failed(slot);
                Err(ModelRuntimeError::Internal(format!(
                    "model startup task failed: {join_error}"
                )))
            }
        }
    }

    /// Stop and forget whatever is in `slot`.
    async fn recycle(&self, slot: usize, core: &Arc<RwLock<GuiProcessCore>>) {
        if let Some(previous) = self.queue.evict(slot) {
            let mut core_w = core.write().await;
            if let Err(e) = core_w.kill(previous.model_id).await {
                warn!(error = %e, "Failed to stop resident model cleanly, continuing");
            }
        }
    }

    /// The primary slot's resident, projected as a routing target.
    pub(super) fn current_model(&self) -> Option<RunningTarget> {
        self.queue.primary().map(target_of)
    }

    /// Stop the primary resident, if there is one.
    pub(super) async fn stop_primary(
        &self,
        core: &Arc<RwLock<GuiProcessCore>>,
    ) -> Result<(), ModelRuntimeError> {
        if let Some(previous) = self.queue.evict(PRIMARY_SLOT) {
            let mut core_w = core.write().await;
            core_w
                .kill(previous.model_id)
                .await
                .map_err(|e| ModelRuntimeError::Internal(e.to_string()))?;
        }
        Ok(())
    }
}

/// Project a resident onto the routing type callers outside this crate speak.
fn target_of(resident: Resident) -> RunningTarget {
    RunningTarget::local(
        resident.port,
        resident.model_id,
        resident.model_name,
        resident.context_size,
        false,
    )
    .with_slot_restore_supported(resident.slot_restore_supported)
    .with_model_sampling(resident.model_sampling)
    .with_cache_ram_health(resident.cache_ram_health)
}

#[cfg(test)]
#[path = "residency_tests.rs"]
mod residency_tests;
