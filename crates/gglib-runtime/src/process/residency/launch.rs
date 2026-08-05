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

use gglib_core::domain::classify_cache_ram;
use gglib_core::paths::slot_model_prefix;
use gglib_core::ports::{AdmissionLease, ModelLaunchSpec, ModelRuntimeError, RunningTarget};
use gglib_core::server_config::{CacheRamSetting, ContextSizeSource, ServerConfigOptions};
use tokio::sync::RwLock;
use tracing::{info, warn};

use super::vram;
use crate::launch_narration::NarrationInputs;
use crate::process::admission::{AdmissionQueue, PRIMARY_SLOT, Resident};
use crate::process::core::GuiProcessCore;
use crate::process::health::wait_for_http_health;
use crate::server_config::build_server_config_narrated;

/// Everything one launch needs, resolved before it is spawned.
///
/// A struct rather than nine parameters because every field is decided at a
/// different point in `admit`, and threading them positionally through a
/// `tokio::spawn` boundary made the ordering easy to get silently wrong.
pub(super) struct LaunchRequest {
    /// The model to launch, already resolved from the catalog.
    pub spec: ModelLaunchSpec,
    /// Options for this launch: template ⊕ per-call ⊕ context chain.
    pub opts: ServerConfigOptions,
    /// The context size `opts` resolves to, and where it came from.
    pub context: (u64, ContextSizeSource),
    /// Which resident slot this launch is claiming.
    pub slot: usize,
    /// Model id to stop before spawning, when the slot is occupied.
    pub evict: Option<u32>,
    /// How to size the host-RAM prompt cache.
    pub cache_ram: CacheRamSetting,
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
    request: LaunchRequest,
) -> Result<(RunningTarget, AdmissionLease), ModelRuntimeError> {
    match launch(&core, &queue, &request).await {
        Ok(outcome) => Ok(outcome),
        Err(e) => {
            queue.launch_failed(request.slot);
            Err(e)
        }
    }
}

#[allow(clippy::too_many_lines)]
async fn launch(
    core: &Arc<RwLock<GuiProcessCore>>,
    queue: &Arc<AdmissionQueue>,
    request: &LaunchRequest,
) -> Result<(RunningTarget, AdmissionLease), ModelRuntimeError> {
    let LaunchRequest {
        spec,
        opts,
        context: (resolved_ctx, ctx_source),
        slot,
        evict,
        cache_ram: cache_ram_setting,
    } = request;
    let mut opts = opts.clone();

    let model_path = &spec.file_path;
    if !tokio::fs::try_exists(model_path).await.unwrap_or(false) {
        return Err(ModelRuntimeError::ModelFileNotFound(
            model_path.display().to_string(),
        ));
    }

    // --- Stop whatever this launch is displacing ---
    //
    // The queue has already decided this is safe: a slot is only offered for
    // eviction once it has no requests in flight.
    if let Some(model_id) = evict {
        if let Some(previous) = queue.evict(*slot) {
            info!(
                model_id = %previous.model_id,
                model_name = %previous.model_name,
                slot = %slot,
                "Stopping resident model for swap"
            );
        }
        let mut core_w = core.write().await;
        if let Err(e) = core_w.kill(*model_id).await {
            warn!(error = %e, "Failed to stop displaced model cleanly, continuing");
        }
    }

    {
        let mut core_w = core.write().await;
        core_w.cleanup_dead().await;
    }

    info!(
        model_id = %spec.id,
        model_name = %spec.name,
        context = %resolved_ctx,
        slot = %slot,
        "Starting model"
    );

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
        model_path.to_path_buf(),
        0, // base_port unused — GuiProcessCore resolves the port itself
        &spec.tags,
        opts,
    );

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
    crate::proxy::banner::print_launch_narration(&narration);

    // Purge stale slot .bin files before spawning a fresh instance: old slot
    // files are incompatible with the new server process. Namespaced by model
    // id, so a co-resident model's caches are untouched.
    if let Some(ref slot_dir) = slot_save_path {
        if let Err(e) = std::fs::create_dir_all(slot_dir) {
            warn!("Failed to create slot directory: {}", e);
        }
        purge_stale_slot_bin_files(slot_dir, spec.id);
    }

    let port = {
        let mut core_w = core.write().await;
        core_w
            .spawn(config)
            .await
            .map_err(|e| ModelRuntimeError::SpawnFailed(e.to_string()))?
    };

    if let Err(e) = wait_for_http_health(port, 120).await {
        return Err(ModelRuntimeError::HealthCheckFailed(e.to_string()));
    }

    let lease = queue.install(
        *slot,
        Resident {
            model_id: spec.id,
            model_name: spec.name.clone(),
            context_size: *resolved_ctx,
            port,
            model_path: spec.file_path.clone(),
            slot_restore_supported: slot_restore.enabled,
            cache_ram_health,
            narration: Some(narration.clone()),
            inflight: 0,
            resident_since: tokio::time::Instant::now(),
            weights_bytes: spec.file_size_bytes,
        },
    );

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
    .with_slot_restore_supported(slot_restore.enabled)
    .with_cache_ram_health(cache_ram_health)
    .with_narration(narration);

    Ok((target, lease))
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

    /// Regression: model `1`'s purge prefix (`1__`) must not delete model
    /// `11`'s files (`11__…`) — the `__` delimiter is what prevents this.
    /// With two models resident at once this matters more than it used to.
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
