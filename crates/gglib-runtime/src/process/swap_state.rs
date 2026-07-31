//! State and startup driver for the single-model-at-a-time strategy.
//!
//! Split out of `manager.rs` so that [`ProcessManager`](super::ProcessManager)
//! is left doing dispatch while the swap strategy owns both its state *and*
//! the launch sequence that mutates it — the two were previously separated by
//! several hundred lines of unrelated code.
//!
//! ## The launch options template
//!
//! [`SwapState`] carries a standing [`ServerConfigOptions`] rather than a
//! hand-picked list of cache fields. Every launch resolves to:
//!
//! ```text
//! template  ⊕  per-call overrides  ⊕  this request's context chain
//! ```
//!
//! where `⊕` is [`ServerConfigOptions::overlay`]. That matters for more than
//! tidiness: a flag added to `ServerConfigOptions` now reaches llama-server
//! through this path with no change here at all. Before, each new flag needed
//! its own field on the strategy, its own constructor parameter, and its own
//! line in the driver — which is exactly why `--mlock` never reached the
//! proxy launch path and `gglib serve` grew a parallel command builder.

use std::path::Path;
use std::sync::Arc;

use gglib_core::domain::{CacheRamHealth, classify_cache_ram};
use gglib_core::paths::slot_model_prefix;
use gglib_core::ports::{
    CatalogError, LaunchOverrides, ModelCatalogPort, ModelRuntimeError, RunningTarget,
};
use gglib_core::server_config::{CacheRamSetting, ServerConfigOptions, resolve_context_size};
use std::path::PathBuf;
use tokio::sync::RwLock;
use tracing::{info, warn};

use super::core::GuiProcessCore;
use super::health::{check_http_health, wait_for_http_health};
use crate::process::startup_guard::StartupState;
use crate::server_config::build_server_config;

/// Currently running model state for the swap strategy.
#[derive(Debug, Clone)]
pub struct CurrentModelState {
    /// Database ID of the running model.
    pub model_id: u32,
    /// Model name.
    pub model_name: String,
    /// Context size being used.
    pub context_size: u64,
    /// Port the server is listening on.
    pub port: u16,
    /// Path to the model file.
    pub model_path: PathBuf,
    /// Whether disk slot restore can resume this model (see
    /// [`gglib_core::ports::RunningTarget::slot_restore_supported`]). Derived
    /// from the launch spec at spawn and cached here so the already-running
    /// fast path can answer without a second catalog lookup.
    pub slot_restore_supported: bool,
    /// Health of the `--cache-ram` budget this instance launched with (see
    /// [`gglib_core::ports::RunningTarget::cache_ram_health`]). Cached for the
    /// same reason: the budget arithmetic only happens at spawn, so a later
    /// `current_model()` call has no way to recompute it.
    pub cache_ram_health: CacheRamHealth,
}

/// Everything the single-model-at-a-time strategy owns.
///
/// Held by [`ProcessStrategy::SingleSwap`](super::ProcessStrategy::SingleSwap).
pub struct SwapState {
    /// Model catalog for resolving model names into launch specifications.
    pub(super) catalog: Arc<dyn ModelCatalogPort>,
    /// Currently running model (`Arc` for `'static` spawn compatibility).
    pub(super) current: Arc<RwLock<Option<CurrentModelState>>>,
    /// Loading slot — `Some` means a driver is active, `None` means idle.
    pub(super) loading: Arc<std::sync::RwLock<Option<StartupState>>>,
    /// Standing launch options every spawn starts from. See the
    /// [module docs](self) for how it composes with per-call overrides.
    pub(super) launch_overrides: ServerConfigOptions,
    /// How to size llama-server's host-RAM prompt cache (`--cache-ram`).
    ///
    /// Not part of [`Self::launch_overrides`] because it is not a flag to
    /// pass through: it is resolved at spawn against live system RAM, the
    /// model's weights and its KV footprint at the launch context size.
    pub(super) cache_ram: CacheRamSetting,
    /// The one model this state will serve, if pinned.
    ///
    /// `Some(name)` makes every other model a hard error instead of a swap —
    /// see [`Self::check_pinned`]. `None` is the ordinary auto-swapping proxy.
    pub(super) pinned: Option<String>,
}

/// Resolve one launch's [`ServerConfigOptions`] and context size together.
///
/// Pulled out of [`SwapState::startup_future`]'s async body so the
/// context-resolution logic can be tested without spawning a process — this
/// is the exact computation that was previously duplicated as a wrong
/// `effective_ctx` (`num_ctx.unwrap_or(default_ctx)`, skipping
/// `model_server_ctx` entirely) and a separate, correct
/// `resolve_context_size` call used only for the KV-cache estimate. See
/// #685.
///
/// The context chain is assigned onto the overlaid template rather than
/// overlaid itself: the manager is authoritative for all three rungs, and a
/// stale `model_server_ctx` inherited from `template` would silently size
/// the launch for a different model than `model_server_ctx` names here.
fn resolve_launch_opts(
    template: &ServerConfigOptions,
    per_call: &ServerConfigOptions,
    num_ctx: Option<u64>,
    default_ctx: u64,
    model_server_ctx: Option<usize>,
) -> (ServerConfigOptions, u64) {
    let mut opts = template.overlay(per_call);
    opts.context_size = num_ctx.or(opts.context_size);
    opts.model_server_ctx = model_server_ctx;
    opts.global_default_ctx = Some(default_ctx);
    let resolved_ctx = resolve_context_size(&opts);
    (opts, resolved_ctx)
}

impl SwapState {
    /// Create auto-swapping swap state with a standing options template.
    pub(super) fn new(
        catalog: Arc<dyn ModelCatalogPort>,
        launch_overrides: ServerConfigOptions,
        cache_ram: CacheRamSetting,
    ) -> Self {
        Self {
            catalog,
            current: Arc::new(RwLock::new(None)),
            loading: Arc::new(std::sync::RwLock::new(None)),
            launch_overrides,
            cache_ram,
            pinned: None,
        }
    }

    /// Create swap state pinned to a single model.
    pub(super) fn pinned_to(
        model_name: impl Into<String>,
        catalog: Arc<dyn ModelCatalogPort>,
        launch_overrides: ServerConfigOptions,
        cache_ram: CacheRamSetting,
    ) -> Self {
        Self {
            pinned: Some(model_name.into()),
            ..Self::new(catalog, launch_overrides, cache_ram)
        }
    }

    /// The model this state is pinned to, if any.
    ///
    /// The read side of [`Self::check_pinned`]: callers that want to avoid
    /// provoking a mismatch rather than handle one need to know the name up
    /// front.
    pub(super) fn pinned_name(&self) -> Option<&str> {
        self.pinned.as_deref()
    }

    /// Reject a request for any model other than the pinned one.
    ///
    /// Checked before the startup guard is consulted, so a foreign request
    /// fails immediately rather than queueing behind — or worse, displacing —
    /// the pinned model.
    pub(super) fn check_pinned(&self, model_name: &str) -> Result<(), ModelRuntimeError> {
        match &self.pinned {
            Some(expected) if expected != model_name => {
                Err(ModelRuntimeError::PinnedModelMismatch {
                    expected: expected.clone(),
                    requested: model_name.to_owned(),
                })
            }
            _ => Ok(()),
        }
    }

    /// Build the detached future that performs one model startup.
    ///
    /// Returned rather than awaited because the caller hands it to
    /// [`drive`](crate::process::startup_guard::drive), which spawns it
    /// detached from any single request's future — so one client
    /// disconnecting cannot abort a launch other callers are waiting on.
    /// Everything it needs is cloned in here, giving the `'static` bound.
    pub(super) fn startup_future(
        &self,
        core: Arc<RwLock<GuiProcessCore>>,
        model_name: String,
        num_ctx: Option<u64>,
        default_ctx: u64,
        overrides: LaunchOverrides,
    ) -> impl std::future::Future<Output = Result<RunningTarget, ModelRuntimeError>> + Send + 'static
    {
        let catalog = Arc::clone(&self.catalog);
        let current = Arc::clone(&self.current);
        let template = self.launch_overrides.clone();
        let cache_ram_setting = overrides.cache_ram.unwrap_or(self.cache_ram);
        let per_call = overrides.options;

        async move {
            // --- Model resolution ---
            let launch_spec = catalog
                .resolve_for_launch(&model_name)
                .await
                .map_err(|e| match e {
                    CatalogError::QueryFailed(msg) | CatalogError::Internal(msg) => {
                        ModelRuntimeError::Internal(msg)
                    }
                })?
                .ok_or_else(|| ModelRuntimeError::ModelNotFound(model_name.clone()))?;

            let model_path = &launch_spec.file_path;

            if !tokio::fs::try_exists(model_path).await.unwrap_or(false) {
                return Err(ModelRuntimeError::ModelFileNotFound(
                    model_path.display().to_string(),
                ));
            }

            // Resolved once, here, before anything else runs. This is the
            // single source of truth for "what context is this launch/reuse
            // decision about" — read by the cache-reuse check below, every
            // log line, the tracked `CurrentModelState`, and the
            // `RunningTarget` returned to the proxy. It used to be computed
            // twice: an `effective_ctx` here that skipped the per-model tier
            // (`num_ctx.unwrap_or(default_ctx)`), and a separate
            // `resolve_context_size` call further down for the KV-cache
            // estimate that did apply it. The two only agreed when no
            // per-model context override was configured; with one
            // configured, everything upstream of the actual `-c` flag
            // — the reuse decision, `/v1/models`, and the proxy's
            // prompt-truncation budget — silently reported the wrong number.
            let (mut opts, resolved_ctx) = resolve_launch_opts(
                &template,
                &per_call,
                num_ctx,
                default_ctx,
                launch_spec
                    .server_defaults
                    .as_ref()
                    .and_then(|sc| sc.context_length),
            );

            // --- Cached instance check (fast path: already running + healthy) ---
            let cached = {
                let guard = current.read().await;
                guard.as_ref().and_then(|c| {
                    (c.model_id == launch_spec.id && c.context_size == resolved_ctx)
                        .then(|| (c.port, c.model_id, c.model_name.clone(), c.context_size))
                })
            };
            if let Some((port, model_id, cached_name, context_size)) = cached {
                if check_http_health(port).await {
                    info!(
                        model_id = %model_id,
                        model_name = %cached_name,
                        port = %port,
                        context = %context_size,
                        "Model already running with correct context"
                    );
                    return Ok(RunningTarget::local(
                        port,
                        model_id,
                        cached_name,
                        context_size,
                        false, // cached healthy — not a fresh spawn
                    )
                    // Same model id as `launch_spec`, so its metadata answers
                    // this without re-reading the cached state.
                    .with_slot_restore_supported(
                        crate::llama::args::resolve_slot_restore(launch_spec.kv_memory_is_partial)
                            .enabled,
                    ));
                }
                warn!(
                    model_id = %model_id,
                    port = %port,
                    "cached model failed health check; recycling degraded instance"
                );
            }

            // --- Stop current model if running ---
            {
                let mut guard = current.write().await;
                if let Some(previous) = guard.take() {
                    info!(
                        model_id = %previous.model_id,
                        model_name = %previous.model_name,
                        "Stopping current model for swap"
                    );
                    let mut core_w = core.write().await;
                    if let Err(e) = core_w.kill(previous.model_id).await {
                        warn!(error = %e, "Failed to stop current model cleanly, continuing");
                    }
                }
            }

            {
                let mut core_w = core.write().await;
                core_w.cleanup_dead().await;
            }

            // --- Spawn new instance ---
            info!(
                model_id = %launch_spec.id,
                model_name = %launch_spec.name,
                context = %resolved_ctx,
                "Starting model"
            );

            // Resolve K/V cache types up front (rather than leaving it to
            // `build_server_config`) so the RAM budget below reflects the
            // *actual* quantized footprint the launch will use. Writing them
            // back as if explicit makes the later resolution a pass-through.
            let kv_types =
                crate::llama::args::resolve_kv_cache_types(opts.cache_type_k, opts.cache_type_v);
            if let Some(explanation) = kv_types.explain() {
                info!("{explanation}");
            }
            opts.cache_type_k = Some(kv_types.k);
            opts.cache_type_v = Some(kv_types.v);

            // Size the host-RAM prompt cache against `resolved_ctx` — the
            // same value `build_server_config` below resolves independently
            // from the same `opts`, so the KV estimate matches the context
            // the server actually launches with by construction.
            let kv_bytes_per_token = launch_spec
                .kv_elems_per_token
                .map(|elems| gglib_core::domain::kv_bytes_per_token(elems, kv_types.k, kv_types.v));
            let cache_ram = crate::llama::args::resolve_cache_ram(
                cache_ram_setting,
                crate::system::total_system_ram_bytes(),
                launch_spec.file_size_bytes,
                kv_bytes_per_token,
                resolved_ctx,
            );
            if let Some(explanation) = cache_ram.explain() {
                info!("{explanation}");
            }
            opts.cache_ram_mb = cache_ram.cache_ram_mb;

            // Classify the budget while the auto-vs-explicit distinction is
            // still in scope — downstream only sees the number, which cannot
            // distinguish a zero the user asked for from one the machine
            // forced.
            let cache_ram_health = classify_cache_ram(
                cache_ram.cache_ram_mb,
                cache_ram.source == crate::llama::args::CacheRamSource::Explicit,
            );

            // Whether the disk slot layer can resume this model at all.
            // Resolved once per spawn alongside the other launch decisions and
            // carried on the target, so the proxy never re-derives it per
            // request.
            let slot_restore =
                crate::llama::args::resolve_slot_restore(launch_spec.kv_memory_is_partial);
            if let Some(explanation) = slot_restore.explain() {
                info!("{explanation}");
            }

            let slot_save_path = opts.slot_save_path.clone();

            let config = build_server_config(
                i64::from(launch_spec.id),
                launch_spec.name.clone(),
                model_path.to_path_buf(),
                0, // base_port unused — GuiProcessCore resolves the port itself
                &launch_spec.tags,
                opts,
            );

            // Purge stale slot .bin files before spawning a fresh instance:
            // old slot files are incompatible with the new server process.
            if let Some(ref slot_dir) = slot_save_path {
                // Ensure the directory exists so llama-server doesn't crash on startup.
                if let Err(e) = std::fs::create_dir_all(slot_dir) {
                    warn!("Failed to create slot directory: {}", e);
                }
                purge_stale_slot_bin_files(slot_dir, launch_spec.id);
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

            // --- SUCCESS: update current model state ---
            {
                let mut guard = current.write().await;
                *guard = Some(CurrentModelState {
                    model_id: launch_spec.id,
                    model_name: launch_spec.name.clone(),
                    context_size: resolved_ctx,
                    port,
                    model_path: launch_spec.file_path.clone(),
                    slot_restore_supported: slot_restore.enabled,
                    cache_ram_health,
                });
            }

            info!(
                model_id = %launch_spec.id,
                model_name = %launch_spec.name,
                port = %port,
                context = %resolved_ctx,
                "Model started successfully"
            );

            Ok(RunningTarget::local(
                port,
                launch_spec.id,
                launch_spec.name,
                resolved_ctx,
                true, // fresh spawn — cache slots are stale
            )
            .with_slot_restore_supported(slot_restore.enabled)
            .with_cache_ram_health(cache_ram_health))
        }
    }
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
    use async_trait::async_trait;
    use gglib_core::ports::{CatalogError, ModelLaunchSpec, ModelSummary};

    #[derive(Debug)]
    struct StubCatalog;

    #[async_trait]
    impl ModelCatalogPort for StubCatalog {
        async fn list_models(&self) -> Result<Vec<ModelSummary>, CatalogError> {
            Ok(Vec::new())
        }
        async fn resolve_model(&self, _name: &str) -> Result<Option<ModelSummary>, CatalogError> {
            Ok(None)
        }
        async fn resolve_for_launch(
            &self,
            _name: &str,
        ) -> Result<Option<ModelLaunchSpec>, CatalogError> {
            Ok(None)
        }
    }

    fn pinned_state(model: &str) -> SwapState {
        SwapState::pinned_to(
            model,
            Arc::new(StubCatalog),
            ServerConfigOptions::default(),
            CacheRamSetting::Auto,
        )
    }

    fn swapping_state() -> SwapState {
        SwapState::new(
            Arc::new(StubCatalog),
            ServerConfigOptions::default(),
            CacheRamSetting::Auto,
        )
    }

    #[test]
    fn pinned_state_admits_its_own_model() {
        assert!(pinned_state("qwen2.5").check_pinned("qwen2.5").is_ok());
    }

    #[test]
    fn pinned_state_rejects_a_foreign_model() {
        let err = pinned_state("qwen2.5")
            .check_pinned("llama-3-8b")
            .expect_err("a foreign model must be refused");

        match err {
            ModelRuntimeError::PinnedModelMismatch {
                expected,
                requested,
            } => {
                assert_eq!(expected, "qwen2.5");
                assert_eq!(requested, "llama-3-8b");
            }
            other => panic!("expected PinnedModelMismatch, got {other:?}"),
        }
    }

    /// Matching is exact: a pinned endpoint must not quietly accept a
    /// near-miss and serve a different model than the caller named.
    #[test]
    fn pinned_matching_is_exact() {
        let state = pinned_state("qwen2.5");
        assert!(state.check_pinned("Qwen2.5").is_err(), "case differs");
        assert!(state.check_pinned("qwen2.5-coder").is_err(), "suffix added");
        assert!(state.check_pinned("qwen2").is_err(), "prefix only");
    }

    /// The unpinned proxy must keep swapping freely — pinning is opt-in.
    #[test]
    fn unpinned_state_admits_any_model() {
        let state = swapping_state();
        assert!(state.check_pinned("anything").is_ok());
        assert!(state.check_pinned("something-else").is_ok());
    }

    /// Pinning changes only the admission check; the launch configuration a
    /// pinned server uses must be identical to the swapping one.
    #[test]
    fn pinning_does_not_alter_launch_configuration() {
        let template = ServerConfigOptions {
            mlock: Some(true),
            cache_reuse: Some(256),
            ..Default::default()
        };
        let state = SwapState::pinned_to(
            "qwen2.5",
            Arc::new(StubCatalog),
            template.clone(),
            CacheRamSetting::ExplicitMb(4096),
        );

        assert_eq!(state.launch_overrides.mlock, template.mlock);
        assert_eq!(state.launch_overrides.cache_reuse, template.cache_reuse);
        assert_eq!(state.cache_ram, CacheRamSetting::ExplicitMb(4096));
    }

    // ── resolve_launch_opts (#685: the context-bookkeeping desync) ─────────

    /// The actual regression: with no per-request `num_ctx` override, a
    /// per-model `server_defaults.context_length` must still win over the
    /// global default — this is what `effective_ctx` used to skip entirely
    /// (`num_ctx.unwrap_or(default_ctx)` never consulted it), so the tracked
    /// context silently disagreed with what `build_server_config` actually
    /// launched llama-server with.
    #[test]
    fn resolve_launch_opts_applies_the_per_model_context_when_nothing_more_specific_is_set() {
        let (opts, resolved_ctx) = resolve_launch_opts(
            &ServerConfigOptions::default(),
            &ServerConfigOptions::default(),
            None,          // no per-request override
            131_072,       // global default
            Some(196_608), // per-model server_defaults.context_length
        );

        assert_eq!(resolved_ctx, 196_608, "per-model tier must win over global");
        // `opts` is exactly what `build_server_config` resolves from — same
        // fields must still be set on it, or the two callers could diverge.
        assert_eq!(opts.model_server_ctx, Some(196_608));
        assert_eq!(opts.global_default_ctx, Some(131_072));
    }

    /// A per-request `num_ctx` still outranks the per-model tier — the
    /// 4-level chain (`resolve_context_size`) is unchanged by this
    /// refactor, only how many times it runs and who reads the result.
    #[test]
    fn resolve_launch_opts_explicit_num_ctx_beats_the_per_model_tier() {
        let (_, resolved_ctx) = resolve_launch_opts(
            &ServerConfigOptions::default(),
            &ServerConfigOptions::default(),
            Some(8_192),
            131_072,
            Some(196_608),
        );

        assert_eq!(resolved_ctx, 8_192);
    }

    /// With nothing else set, the global default is what's left.
    #[test]
    fn resolve_launch_opts_falls_back_to_the_global_default() {
        let (_, resolved_ctx) = resolve_launch_opts(
            &ServerConfigOptions::default(),
            &ServerConfigOptions::default(),
            None,
            131_072,
            None,
        );

        assert_eq!(resolved_ctx, 131_072);
    }

    /// A stale `model_server_ctx` left over on `template` from a previous
    /// model must not leak into this launch — `model_server_ctx` is always
    /// assigned from this call's own parameter, never overlaid.
    #[test]
    fn resolve_launch_opts_never_inherits_a_stale_model_server_ctx_from_the_template() {
        let stale_template = ServerConfigOptions {
            model_server_ctx: Some(4_096), // some other model's context
            ..Default::default()
        };

        let (opts, resolved_ctx) = resolve_launch_opts(
            &stale_template,
            &ServerConfigOptions::default(),
            None,
            131_072,
            None,
        );

        assert_eq!(
            opts.model_server_ctx, None,
            "must not inherit the stale value"
        );
        assert_eq!(resolved_ctx, 131_072);
    }

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
