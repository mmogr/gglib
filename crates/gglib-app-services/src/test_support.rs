//! Shared mock infrastructure for gglib-app-services unit tests.
//!
//! All types are `pub(crate)` and only compiled under `#[cfg(test)]`
//! (the module is declared with `#[cfg(test)] mod test_support;` in lib.rs).

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use gglib_core::download::{DownloadError, DownloadId, QueueSnapshot};
use gglib_core::events::AppEvent;
use gglib_core::ports::{
    Admission, AppEventEmitter, DownloadManagerPort, LaunchOverrides, ModelRuntimeError,
    ModelRuntimePort, ProcessHandle, RunningTarget, SystemProbePort, ToolSupportDetection,
    ToolSupportDetectionInput, ToolSupportDetectorPort,
};
use gglib_core::services::AppCore;
use gglib_core::utils::system::{Dependency, GpuInfo, SystemMemoryInfo};
use gglib_db::{CoreFactory, setup_test_database};

pub(crate) use crate::test_support_hf::MockHfClient;

// ---------------------------------------------------------------------------
// RecordingEmitter
// ---------------------------------------------------------------------------

/// An emitter that keeps what it was told, in the order it was told.
///
/// The one recording emitter this crate's tests share. "Emitted nothing" is
/// as much a claim worth asserting as "emitted this": a refused mutation
/// that still announced itself would be a lie no return value catches.
#[derive(Default)]
pub(crate) struct RecordingEmitter(Mutex<Vec<AppEvent>>);

impl RecordingEmitter {
    /// Everything emitted so far, oldest first.
    pub(crate) fn events(&self) -> Vec<AppEvent> {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }
}

impl AppEventEmitter for RecordingEmitter {
    fn emit(&self, event: AppEvent) {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(event);
    }
}

// ---------------------------------------------------------------------------
// MockDownloadManager
// ---------------------------------------------------------------------------

/// A controllable stub for `DownloadManagerPort`.
///
/// - `fail_cancel = true` → `cancel_download` returns `DownloadError::NotFound`
/// - `reorder_position` → the position value returned by `reorder_queue`
/// - `calls` → the cancel, remove and clear calls it was sent, in order
pub(crate) struct MockDownloadManager {
    pub fail_cancel: bool,
    pub reorder_position: u32,
    pub calls: Arc<Mutex<Vec<String>>>,
}

impl Default for MockDownloadManager {
    fn default() -> Self {
        Self {
            fail_cancel: false,
            reorder_position: 1,
            calls: Arc::default(),
        }
    }
}

impl MockDownloadManager {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// A variant whose `cancel_download` always returns `NotFound`.
    pub(crate) fn failing_cancel() -> Self {
        Self {
            fail_cancel: true,
            ..Self::default()
        }
    }

    /// Keep that `call` was sent.
    fn record(&self, call: String) {
        self.calls.lock().unwrap().push(call);
    }
}

#[async_trait]
impl DownloadManagerPort for MockDownloadManager {
    async fn queue_smart(
        self: Arc<Self>,
        _repo_id: String,
        _quantization: Option<String>,
    ) -> Result<DownloadId, DownloadError> {
        Ok(DownloadId::new("mock/model", Some("Q8_0")))
    }

    async fn get_queue_snapshot(&self) -> Result<QueueSnapshot, DownloadError> {
        Ok(QueueSnapshot::default())
    }

    async fn cancel_download(&self, id: &DownloadId) -> Result<(), DownloadError> {
        self.record(format!("cancel_download {id}"));
        if self.fail_cancel {
            Err(DownloadError::NotFound {
                message: id.to_string(),
            })
        } else {
            Ok(())
        }
    }

    async fn cancel_all(&self) -> Result<(), DownloadError> {
        Ok(())
    }

    async fn active_count(&self) -> Result<u32, DownloadError> {
        Ok(0)
    }

    async fn remove_from_queue(&self, id: &DownloadId) -> Result<(), DownloadError> {
        self.record(format!("remove_from_queue {id}"));
        Ok(())
    }

    async fn reorder_queue(
        &self,
        _id: &DownloadId,
        _new_position: u32,
    ) -> Result<u32, DownloadError> {
        Ok(self.reorder_position)
    }

    async fn set_max_queue_size(&self, _size: u32) -> Result<(), DownloadError> {
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// MockToolSupportDetector
// ---------------------------------------------------------------------------

/// A `ToolSupportDetectorPort` that always reports no tool calling support.
pub(crate) struct MockToolSupportDetector;

impl ToolSupportDetectorPort for MockToolSupportDetector {
    fn detect(&self, _input: ToolSupportDetectionInput<'_>) -> ToolSupportDetection {
        ToolSupportDetection {
            supports_tool_calling: false,
            confidence: 0.0,
            detected_format: None,
        }
    }
}

// ---------------------------------------------------------------------------
// MockSystemProbePort
// ---------------------------------------------------------------------------

/// A `SystemProbePort` that returns configurable memory and empty deps/GPU info.
#[allow(dead_code)]
pub(crate) struct MockSystemProbePort {
    pub total_ram_bytes: u64,
}

impl Default for MockSystemProbePort {
    fn default() -> Self {
        Self {
            total_ram_bytes: 16 * 1024 * 1024 * 1024, // 16 GiB
        }
    }
}

impl SystemProbePort for MockSystemProbePort {
    fn check_all_dependencies(&self) -> Vec<Dependency> {
        vec![]
    }

    fn detect_gpu_info(&self) -> GpuInfo {
        GpuInfo {
            has_nvidia_gpu: false,
            cuda_version: None,
            has_metal: false,
            has_vulkan: false,
            vulkan_headers: false,
            vulkan_glslc: false,
            vulkan_spirv_headers: false,
        }
    }

    fn get_system_memory_info(&self) -> SystemMemoryInfo {
        SystemMemoryInfo {
            total_ram_bytes: self.total_ram_bytes,
            gpu_memory_bytes: None,
            is_unified_memory: false,
            has_nvidia_gpu: false,
        }
    }
}

// ---------------------------------------------------------------------------
// RunningRuntime
// ---------------------------------------------------------------------------

/// A runtime with one model being served, on one port. It keeps whether it
/// was told to stop, and how many times it was asked what is running.
///
/// Stands in for the runtime `ServerOps` starts models through, which is the
/// one `ModelOps` asks what is being served.
#[derive(Debug)]
pub(crate) struct RunningRuntime {
    model_id: i64,
    port: u16,
    stopped: AtomicBool,
    asked: AtomicUsize,
}

impl RunningRuntime {
    pub(crate) fn new(model_id: i64, port: u16) -> Self {
        Self {
            model_id,
            port,
            stopped: AtomicBool::new(false),
            asked: AtomicUsize::new(0),
        }
    }

    pub(crate) fn stopped(&self) -> bool {
        self.stopped.load(Ordering::SeqCst)
    }

    /// How many times it has been asked what is running.
    pub(crate) fn asked(&self) -> usize {
        self.asked.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl ModelRuntimePort for RunningRuntime {
    async fn admit(
        &self,
        _model_name: &str,
        _num_ctx: Option<u64>,
        _default_ctx: Option<u64>,
        _overrides: LaunchOverrides,
    ) -> Result<Admission, ModelRuntimeError> {
        unimplemented!("a removal and an upgrade start nothing")
    }

    async fn current_model(&self) -> Option<RunningTarget> {
        None
    }

    async fn list_running(&self) -> Vec<ProcessHandle> {
        self.asked.fetch_add(1, Ordering::SeqCst);
        vec![ProcessHandle::new(
            self.model_id,
            "running-model".to_string(),
            None,
            self.port,
            0,
        )]
    }

    async fn stop_current(&self) -> Result<(), ModelRuntimeError> {
        self.stopped.store(true, Ordering::SeqCst);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// AppCore test helper
// ---------------------------------------------------------------------------

/// Build an `AppCore` backed by an in-memory `SQLite` database.
///
/// Uses the `test-utils` feature gate from `gglib-db`. Sets this binary's own
/// data root first, as [`test_core_and_proxy_over`] does.
pub(crate) async fn test_core() -> Arc<AppCore> {
    gglib_core::paths::isolate_data_root();
    let pool = setup_test_database().await.expect("in-memory DB");
    Arc::new(AppCore::bare(CoreFactory::build_repos(pool)))
}

/// An `AppCore` and a `ProxyOps` sharing one in-memory database.
///
/// `ServerOps` drives models through the proxy, so its tests need both. The
/// proxy is never started here — the runtime reports nothing running, which is
/// the state the lifecycle tests exercise.
#[allow(dead_code)]
pub(crate) async fn test_core_and_proxy() -> (Arc<AppCore>, Arc<crate::ProxyOps>) {
    let pool = setup_test_database().await.expect("in-memory DB");
    test_core_and_proxy_over(&CoreFactory::build_repos(pool))
}

/// [`test_core_and_proxy`] over repositories the caller built, so a test can
/// put a repository of its own in place of one of them.
///
/// Sets this binary's own data root first, so whatever the core resolves
/// from it, such as the endpoint key an arm writes, stays out of the
/// checkout, where in a debug build an installed daemon keeps its own (#955).
pub(crate) fn test_core_and_proxy_over(
    repos: &gglib_core::ports::Repos,
) -> (Arc<AppCore>, Arc<crate::ProxyOps>) {
    use gglib_core::cache_config::CacheRamSetting;
    use gglib_core::ports::ModelCatalogPort;
    use gglib_core::server_config::ServerConfigOptions;
    use gglib_runtime::ports_impl::{CatalogPortImpl, RuntimePortImpl};
    use gglib_runtime::process::{ProcessManager, RuntimeBinaries};

    gglib_core::paths::isolate_data_root();
    let catalog: Arc<dyn ModelCatalogPort> = Arc::new(CatalogPortImpl::new(repos.models.clone()));
    let runtime = Arc::new(RuntimePortImpl::new(Arc::new(ProcessManager::new(
        9000,
        RuntimeBinaries {
            llama: "llama-server".into(),
            sd: "sd-server".into(),
        },
        catalog,
        ServerConfigOptions::default(),
        CacheRamSetting::Auto,
    ))));
    test_core_and_proxy_on(repos, runtime)
}

/// [`test_core_and_proxy_over`] with the runtime the proxy drives models
/// through supplied too, so a test can script what a start and a stop meet.
pub(crate) fn test_core_and_proxy_on(
    repos: &gglib_core::ports::Repos,
    runtime: Arc<dyn gglib_core::ports::ModelRuntimePort>,
) -> (Arc<AppCore>, Arc<crate::ProxyOps>) {
    gglib_core::paths::isolate_data_root();
    let core = Arc::new(AppCore::bare(repos.clone()));
    let proxy = Arc::new(crate::proxy::ProxyOps::new(crate::proxy::ProxyDeps {
        supervisor: Arc::new(gglib_runtime::proxy::ProxySupervisor::new()),
        model_repo: repos.models.clone(),
        mcp: Arc::new(gglib_mcp::McpService::new(repos.mcp_servers.clone())),
        core: Arc::clone(&core),
        runtime,
    }));
    (core, proxy)
}
