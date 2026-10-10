//! One model launch, start to finish.
//!
//! Everything model-static has already been resolved by the time this runs (see
//! the [module docs](super)), so what is left is a straight line: stop what is
//! being displaced, size the caches, narrate the decisions, spawn, wait for
//! health, record the result.
//!
//! The whole sequence runs inside a detached `tokio::spawn`, which is what makes
//! a client disconnect harmless: the launch other requests are waiting on
//! completes regardless of whether the request that triggered it is still there
//! to receive it.

use std::path::Path;
use std::sync::Arc;

use gglib_core::cache_config::CacheRamSetting;
use gglib_core::domain::{
    CacheRamHealth, LaunchNarration, RuntimeKind, TemplateCapsState, classify_cache_ram,
};
use gglib_core::paths::slot_model_prefix;
use gglib_core::ports::{
    AdmissionLease, ModelCatalogPort, ModelLaunchSpec, ModelRuntimeError, RunningTarget,
};
use gglib_core::server_config::{ContextSizeSource, ServerConfigOptions};
use tokio::sync::RwLock;
use tracing::{info, warn};

use super::spawned_child::{LIVENESS_TICK, SpawnedChild};
use super::vram;
use crate::launch_narration::NarrationInputs;
use crate::process::SpawnConfig;
use crate::process::admission::{AdmissionQueue, PRIMARY_SLOT, Resident};
use crate::process::core::GuiProcessCore;
use crate::process::health::wait_for_http_health;
use crate::sd::SdServerConfig;
use crate::server_config::build_server_config_narrated;

#[path = "launch_files.rs"]
pub(super) mod launch_files;

/// Everything one launch needs, resolved before it is spawned.
///
/// A struct rather than nine parameters because every field is decided at a
/// different point in `admit`, and threading them positionally through a
/// `tokio::spawn` boundary made the ordering easy to get silently wrong.
pub(super) struct LaunchRequest {
    /// The model to launch, already resolved from the catalog.
    pub spec: ModelLaunchSpec,
    /// Options for this launch: template ⊕ per-call ⊕ context chain. Unread
    /// for an image model.
    pub opts: ServerConfigOptions,
    /// The context size `opts` resolves to, and where it came from. `0` for
    /// an image model, which has none.
    pub context: (u64, ContextSizeSource),
    /// Which resident slot this launch is claiming.
    pub slot: usize,
    /// Model id to stop before spawning, when the slot is occupied.
    pub evict: Option<u32>,
    /// How to size the host-RAM prompt cache.
    pub cache_ram: CacheRamSetting,
    /// How long to wait for this model's server to answer `/health`.
    ///
    /// Resolved once, here, for the same reason every other field is: the
    /// outer launch budget must leave room for this wait, and deriving the two
    /// separately is what let a flat budget silently cap a scaled wait — the
    /// future dropped mid-await, the cleanup skipped, the child leaked. One
    /// value, read by both, and the relationship cannot drift.
    pub health_deadline_secs: u64,
}

/// Run one launch and record it in the queue.
///
/// Returns the routing target and a lease already counted against the new
/// resident, so the model cannot be evicted in the window between finishing its
/// launch and serving the request that paid for it.
///
/// On failure the slot is released, so the next request can try again rather
/// than finding it latched mid-launch forever.
pub(super) async fn run(
    core: Arc<RwLock<GuiProcessCore>>,
    queue: Arc<AdmissionQueue>,
    catalog: Arc<dyn ModelCatalogPort>,
    request: LaunchRequest,
) -> Result<(RunningTarget, AdmissionLease), ModelRuntimeError> {
    match launch(&core, &queue, &request).await {
        Ok(outcome) => {
            // The one moment ADR 0007's observation can be taken: a fresh
            // spawn is health-ready, so its /props describes exactly this
            // binary–model pair. Detached, so a slow read can neither delay
            // the admission this launch owes nor fail it. sd-server has no
            // /props and no chat template to report.
            if request.spec.runtime() == RuntimeKind::Llama {
                tokio::spawn(observe_template_caps(
                    catalog,
                    request.spec.id,
                    outcome.0.base_url.clone(),
                ));
            }
            Ok(outcome)
        }
        Err(e) => {
            queue.launch_failed(request.slot);
            Err(e)
        }
    }
}

/// Read the just-launched server's `chat_template_caps` self-report and
/// persist it on the model's catalog row (ADR 0007, decision 2: snapshotted
/// once per launch).
///
/// Failure is a result, not an error — [`crate::llama::runtime_probe`]'s
/// discipline: an unreadable `/props` records **nothing**, leaving the row at
/// "never observed" rather than manufacturing a negative, and no failure here
/// can affect the launch, which has already returned. The catalog port skips
/// the write when the stored value already matches, so repeat launches of an
/// unchanged pair are read-only.
///
/// Reuses [`gglib_proxy::props::fetch_props`] rather than a second HTTP
/// probe: it is the same endpoint the proxy's baseline check reads, with the
/// same timeout and the same failure-is-a-variant parse.
async fn observe_template_caps(
    catalog: Arc<dyn ModelCatalogPort>,
    model_id: u32,
    base_url: String,
) {
    let client = gglib_proxy::loopback::client();
    let reading = gglib_proxy::props::fetch_props(&client, &base_url).await;
    match reading.caps {
        TemplateCapsState::Read { caps } => {
            if let Err(e) = catalog.record_template_caps(model_id, caps).await {
                warn!(model_id, error = %e, "could not record template caps on the model row");
            }
        }
        TemplateCapsState::Unreadable { reason } => {
            // Not a warning: a pre-caps build lands here on every launch, and
            // "never observed" is a well-defined state every consumer of the
            // tri-state already handles.
            info!(model_id, %reason, "template caps not observed for this launch");
        }
        TemplateCapsState::NotYetRead => {}
    }
}

/// What the runtime-specific half of a launch hands the shared half: the
/// spawn, its narration, and what the resident records about it.
struct Planned {
    config: SpawnConfig,
    narration: LaunchNarration,
    slot_restore_supported: bool,
    cache_ram_health: CacheRamHealth,
}

#[allow(clippy::too_many_lines)]
async fn launch(
    core: &Arc<RwLock<GuiProcessCore>>,
    queue: &Arc<AdmissionQueue>,
    request: &LaunchRequest,
) -> Result<(RunningTarget, AdmissionLease), ModelRuntimeError> {
    let LaunchRequest {
        spec,
        context: (resolved_ctx, ctx_source),
        slot,
        evict,
        health_deadline_secs,
        ..
    } = request;
    let runtime = spec.runtime();

    // --- Stop whatever this launch is displacing, first ---
    //
    // The queue has already decided this is safe: a slot is only offered for
    // eviction once it has no requests in flight. And it has already
    // forgotten the resident: `poll` captured this id and replaced the slot
    // with `Loading` in the same critical section. So nothing may return
    // before this kill. A launch that gave up earlier, on a file gone
    // missing say, freed the slot with the displaced server still running
    // and unknown to the queue, and `spawn`'s "already running" guard then
    // refused its relaunch until the daemon restarted.
    if let Some(model_id) = evict {
        info!(model_id = %model_id, slot = %slot, "Stopping resident model for swap");
        let mut core_w = core.write().await;
        if let Err(e) = core_w.kill(*model_id).await {
            warn!(error = %e, "Failed to stop displaced model cleanly, continuing");
        }
    }

    // The files again, after the stop: `admit` checked them before the
    // queue, and one removed while this request waited fails here, the same
    // way whichever runtime serves it.
    let sd_server = core
        .read()
        .await
        .binary(RuntimeKind::StableDiffusion)
        .to_path_buf();
    launch_files::preflight(spec, &sd_server).await?;

    {
        let mut core_w = core.write().await;
        core_w.cleanup_dead().await;
    }

    info!(
        model_id = %spec.id,
        model_name = %spec.name,
        runtime = %runtime.label(),
        context = %resolved_ctx,
        slot = %slot,
        "Starting model"
    );

    let planned = match runtime {
        RuntimeKind::Llama => plan_llama(queue, request),
        RuntimeKind::StableDiffusion => plan_sd(request)?,
    };
    let Planned {
        config,
        narration,
        slot_restore_supported,
        cache_ram_health,
    } = planned;
    crate::proxy::banner::print_launch_narration(&narration);

    // The guard is armed inside the same critical section that created the
    // child, from the pid `spawn` hands back. Reading the pid afterwards would
    // mean a second lock acquisition with an `await` between it and the spawn
    // — a suspension point at which the child exists and nothing owns it,
    // which is the leak this whole commit is about.
    let (port, mut child) = {
        let mut core_w = core.write().await;
        let (port, pid) = core_w
            .spawn(config)
            .await
            .map_err(|e| ModelRuntimeError::SpawnFailed(e.to_string()))?;
        (port, SpawnedChild::arm(core, spec.id, pid))
    };

    // From here until `queue.install` the child exists and nothing else owns
    // it. `spawn` registered it in `GuiProcessCore::processes`, but the queue
    // has no resident for it yet — and every kill path is gated on
    // `queue.evict` returning one, while `cleanup_dead` reaps only processes
    // that have already exited. A failure in this window therefore left a live
    // server holding VRAM with nobody able to route to it or stop it, and
    // `spawn`'s "already running" guard refused every retry until the daemon
    // was restarted.
    //
    // The guard, not an error arm, because the dominant failure here is
    // *cancellation*: `run_launch` runs inside a `tokio::time::timeout`, which
    // drops the future rather than returning, so an `if let Err(..)` cleanup
    // is skipped entirely on exactly the slow launches this is about.

    let deadline_secs = *health_deadline_secs;
    let started = tokio::time::Instant::now();

    // A launch that fails because gglib chose the context has to say so.
    // Everything else about a startup failure is the model server's business;
    // this one is ours, the remedy is not guessable from the symptom, and the
    // failure repeats identically forever — the budget is a per-machine
    // constant, so a fit too large to load produces the same number on every
    // retry. An image model has no context, so this never speaks for one.
    let blame_the_fit = || {
        if runtime == RuntimeKind::Llama && *ctx_source == ContextSizeSource::FittedToHardware {
            format!(
                " — context {resolved_ctx} was fitted to this machine; set a \
                 global default to override it, or GGLIB_DISABLE_CONTEXT_FIT=1 \
                 to fall back to {}",
                gglib_core::settings::DEFAULT_CONTEXT_SIZE
            )
        } else {
            String::new()
        }
    };

    // Raced against the child's own exit, not just run to the deadline. A
    // server that dies on startup — bad arguments, OOM, a missing GPU library
    // — never answers its health probe, and polling a dead port until a
    // budget sized for a large model runs out would make a failed launch take
    // minutes to report.
    let health = wait_for_http_health(port, deadline_secs, runtime);
    tokio::pin!(health);
    loop {
        tokio::select! {
            result = &mut health => {
                result.map_err(|e| {
                    ModelRuntimeError::HealthCheckFailed(format!("{e}{}", blame_the_fit()))
                })?;
                break;
            }
            () = tokio::time::sleep(LIVENESS_TICK) => {
                // `try_write`, not `write`: this is a best-effort check, and
                // blocking on the lock would stop polling the health future it
                // is supervising while that future's deadline keeps running.
                // A missed tick simply defers to the next one.
                if core.try_write().is_ok_and(|mut c| c.has_exited(spec.id)) {
                    return Err(ModelRuntimeError::HealthCheckFailed(format!(
                        "{} for {} exited during startup{}",
                        runtime.server_name(),
                        spec.name,
                        blame_the_fit()
                    )));
                }
            }
        }
    }

    // Diagnostic, not an instrument: `process::residency` is Tier B, which
    // owes no deletion criterion (ADR 0001). Recorded next to the deadline
    // that allowed it so a person debugging a slow launch can see both.
    info!(
        model_id = %spec.id,
        model_name = %spec.name,
        weights_bytes = %spec.file_size_bytes,
        deadline_secs = %deadline_secs,
        load_secs = %started.elapsed().as_secs(),
        "model became healthy"
    );

    let lease = queue.install(
        *slot,
        Resident {
            model_id: spec.id,
            model_name: spec.name.clone(),
            context_size: *resolved_ctx,
            port,
            projector: spec.projector.clone(),
            runtime,
            components: spec.components.clone(),
            slot_restore_supported,
            model_sampling: spec.model_sampling,
            cache_ram_health,
            narration: Some(narration.clone()),
            inflight: 0,
            resident_since: tokio::time::Instant::now(),
            weights_bytes: spec.file_size_bytes,
        },
    );

    // The queue owns the process now: eviction, recycling and shutdown can all
    // reach it, so the guard must not.
    child.disarm();

    info!(
        model_id = %spec.id,
        model_name = %spec.name,
        port = %port,
        context = %resolved_ctx,
        slot = %slot,
        primary = %(*slot == PRIMARY_SLOT),
        "Model started successfully"
    );

    let target = RunningTarget::local(
        port,
        spec.id,
        spec.name.clone(),
        *resolved_ctx,
        true, // fresh spawn — cache slots are stale
    )
    .with_slot_restore_supported(slot_restore_supported)
    .with_model_sampling(spec.model_sampling)
    .with_cache_ram_health(cache_ram_health)
    .with_runtime(runtime)
    .with_narration(narration);

    Ok((target, lease))
}

/// llama-server's half of a launch: the KV cache types, the host-RAM prompt
/// cache, the disk slot layer, the command line, and the narration of all
/// of it.
fn plan_llama(queue: &AdmissionQueue, request: &LaunchRequest) -> Planned {
    let LaunchRequest {
        spec,
        opts,
        context: (resolved_ctx, ctx_source),
        slot,
        cache_ram: cache_ram_setting,
        ..
    } = request;
    let mut opts = opts.clone();

    // Resolve K/V cache types up front (rather than leaving it to
    // `build_server_config`) so the RAM budget below reflects the *actual*
    // quantized footprint the launch will use. Writing them back as if explicit
    // makes the later resolution a pass-through.
    let kv_types = crate::llama::args::resolve_kv_cache_types(opts.cache_type_k, opts.cache_type_v);
    if let Some(explanation) = kv_types.explain() {
        info!("{explanation}");
    }
    opts.cache_type_k = Some(kv_types.k);
    opts.cache_type_v = Some(kv_types.v);

    // Size the host-RAM prompt cache against `resolved_ctx` — the same value
    // `build_server_config` below resolves independently from the same `opts`,
    // so the KV estimate matches the context the server actually launches with
    // by construction.
    //
    // The RAM figure is what is left after the *other* residents, not the whole
    // machine: with two models loaded, budgeting each against the full total
    // would have them both claim the same memory.
    let kv_bytes_per_token = spec
        .kv_elems_per_token
        .map(|elems| gglib_core::domain::kv_bytes_per_token(elems, kv_types.k, kv_types.v));
    let others: Vec<Resident> = queue
        .residents()
        .into_iter()
        .filter(|(s, _)| s != slot)
        .map(|(_, r)| r)
        .collect();
    let cache_ram = crate::llama::args::resolve_cache_ram(
        *cache_ram_setting,
        vram::ram_available_for(crate::system::total_system_ram_bytes(), &others),
        spec.file_size_bytes,
        kv_bytes_per_token,
        *resolved_ctx,
    );
    if let Some(explanation) = cache_ram.explain() {
        info!("{explanation}");
    }
    opts.cache_ram_mb = cache_ram.cache_ram_mb;

    // Classify the budget while the auto-vs-explicit distinction is still in
    // scope — downstream only sees the number, which cannot distinguish a zero
    // the user asked for from one the machine forced.
    let cache_ram_health = classify_cache_ram(
        cache_ram.cache_ram_mb,
        cache_ram.source == crate::llama::args::CacheRamSource::Explicit,
    );

    // Whether the disk slot layer can resume this model at all. Resolved once
    // per spawn alongside the other launch decisions and carried on the target,
    // so the proxy never re-derives it per request.
    let slot_restore = crate::llama::args::resolve_slot_restore(spec.kv_memory_is_partial);
    if let Some(explanation) = slot_restore.explain() {
        info!("{explanation}");
    }

    let slot_save_path = opts.slot_save_path.clone();

    let (config, capabilities) = build_server_config_narrated(
        i64::from(spec.id),
        spec.name.clone(),
        spec.file_path.clone(),
        0, // base_port unused — GuiProcessCore resolves the port itself
        &spec.tags,
        opts,
    );
    let config = config.with_mmproj(spec.projector.clone());

    // Every decision above is now resolved, so this is the last point at which
    // they all coexist — the narration is assembled here and then carried,
    // never re-derived. Printed before the spawn rather than after the health
    // check so a launch that hangs still tells the user what it was trying to
    // do.
    let narration = crate::launch_narration::narrate(&NarrationInputs {
        spec,
        context: (*resolved_ctx, *ctx_source),
        kv_types,
        cache_ram: &cache_ram,
        disk_cache_enabled: slot_save_path.is_some(),
        slot_restore,
        capabilities: &capabilities,
    });

    // Purge stale slot .bin files before spawning a fresh instance: old slot
    // files are incompatible with the new server process. Namespaced by model
    // id, so a co-resident model's caches are untouched.
    if let Some(ref slot_dir) = slot_save_path {
        if let Err(e) = std::fs::create_dir_all(slot_dir) {
            warn!("Failed to create slot directory: {}", e);
        }
        purge_stale_slot_bin_files(slot_dir, spec.id);
    }

    Planned {
        config: SpawnConfig::Llama(config),
        narration,
        slot_restore_supported: slot_restore.enabled,
        cache_ram_health,
    }
}

/// `sd-server`'s half of a launch: the model's files, its family and the
/// port, and the narration of where it was placed and what it was judged to
/// need. No context, no prompt cache, no disk slots: sd-server has none.
fn plan_sd(request: &LaunchRequest) -> Result<Planned, ModelRuntimeError> {
    let LaunchRequest { spec, slot, .. } = request;
    // A model's runtime is its family, so an sd launch always has one; the
    // footprint is the one the queue placed it by.
    let (Some(family), Some(footprint)) = (spec.image_family, vram::image_footprint(spec)) else {
        return Err(ModelRuntimeError::Internal(format!(
            "'{}' was launched on sd-server without an image family",
            spec.name
        )));
    };
    let narration = crate::launch_narration::narrate_sd(spec, *slot, footprint);
    Ok(Planned {
        config: SpawnConfig::Sd(SdServerConfig {
            model_id: i64::from(spec.id),
            model_name: spec.name.clone(),
            model_path: spec.file_path.clone(),
            family,
            components: spec.components.clone(),
            port: None,
        }),
        narration,
        slot_restore_supported: false,
        cache_ram_health: CacheRamHealth::LlamaDefault,
    })
}

/// Remove stale slot files for the given model from `slot_dir`.
///
/// Slot files are flat as `{slot_dir}/{model_id}__{session}.bin`; this removes
/// only files whose name starts with the model's `{model_id}__` prefix, so a
/// model/context swap leaves other models' caches untouched. Called on
/// llama-server restart when the model or context size changes.
fn purge_stale_slot_bin_files(slot_dir: &Path, model_id: u32) {
    let prefix = slot_model_prefix(model_id);
    if let Ok(entries) = std::fs::read_dir(slot_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let is_bin = path.extension().and_then(|e| e.to_str()) == Some("bin");
            let matches_model = path
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with(&prefix));
            if is_bin && matches_model {
                let _ = std::fs::remove_file(&path);
            }
        }
    }
    // Silently skip if slot_dir doesn't exist or can't be read.
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn temp_dir(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "gglib-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().elapsed().unwrap().as_nanos()
        ))
    }

    #[tokio::test]
    async fn purge_stale_slot_bin_files_removes_bin_only() {
        let dir = temp_dir("purge-test");
        let model_id: u32 = 42;
        tokio::fs::create_dir_all(&dir).await.unwrap();
        // Flat `{model_id}__{session}.bin` files for model 42 (should be purged).
        tokio::fs::write(dir.join("42__session1.bin"), &[0u8; 8])
            .await
            .unwrap();
        tokio::fs::write(dir.join("42__session2.bin"), &[0u8; 8])
            .await
            .unwrap();
        // A non-.bin file with the model prefix (should survive — wrong extension).
        tokio::fs::write(dir.join("42__notes.txt"), "keep me")
            .await
            .unwrap();
        // A legacy pre-namespacing flat .bin without any prefix (should survive).
        tokio::fs::write(dir.join("orphan.bin"), &[0u8; 8])
            .await
            .unwrap();
        // Another model's .bin (should survive).
        tokio::fs::write(dir.join("99__session3.bin"), &[0u8; 8])
            .await
            .unwrap();

        purge_stale_slot_bin_files(&dir, model_id);

        assert!(
            !tokio::fs::try_exists(dir.join("42__session1.bin"))
                .await
                .unwrap()
        );
        assert!(
            !tokio::fs::try_exists(dir.join("42__session2.bin"))
                .await
                .unwrap()
        );
        assert!(
            tokio::fs::try_exists(dir.join("42__notes.txt"))
                .await
                .unwrap()
        );
        assert!(tokio::fs::try_exists(dir.join("orphan.bin")).await.unwrap());
        assert!(
            tokio::fs::try_exists(dir.join("99__session3.bin"))
                .await
                .unwrap()
        );

        let _ = tokio::fs::remove_dir_all(&dir).await;
    }

    /// Model `1`'s purge prefix (`1__`) must not delete model `11`'s files
    /// (`11__…`), which may belong to the other resident — the `__` delimiter
    /// is what prevents this.
    #[tokio::test]
    async fn purge_prefix_does_not_match_longer_model_id() {
        let dir = temp_dir("purge-prefix-test");
        tokio::fs::create_dir_all(&dir).await.unwrap();
        tokio::fs::write(dir.join("1__a.bin"), &[0u8; 8])
            .await
            .unwrap();
        tokio::fs::write(dir.join("11__b.bin"), &[0u8; 8])
            .await
            .unwrap();

        purge_stale_slot_bin_files(&dir, 1);

        assert!(!tokio::fs::try_exists(dir.join("1__a.bin")).await.unwrap());
        assert!(
            tokio::fs::try_exists(dir.join("11__b.bin")).await.unwrap(),
            "model 11's file must survive a purge of model 1"
        );

        let _ = tokio::fs::remove_dir_all(&dir).await;
    }

    #[tokio::test]
    async fn purge_stale_slot_bin_files_noop_on_missing_dir() {
        // Should not panic or error — just returns silently (dir doesn't exist).
        purge_stale_slot_bin_files(&temp_dir("purge-missing"), 999);
    }
}
