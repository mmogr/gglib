//! Shared mock infrastructure for gglib-app-services unit tests.
//!
//! All types are `pub(crate)` and only compiled under `#[cfg(test)]`
//! (the module is declared with `#[cfg(test)] mod test_support;` in lib.rs).

use std::sync::Arc;

use async_trait::async_trait;
use gglib_core::download::{DownloadError, DownloadId, QueueSnapshot};
use gglib_core::ports::{
    DownloadManagerPort, DownloadRequest, HfClientPort, HfFileInfo, HfPortError, HfQuantInfo,
    HfRepoInfo, HfSearchOptions, HfSearchResult, ProcessError, ProcessHandle, ProcessRunner,
    ServerConfig, ServerHealth, SystemProbePort, ToolSupportDetection, ToolSupportDetectionInput,
    ToolSupportDetectorPort,
};
use gglib_core::services::AppCore;
use gglib_core::utils::system::{Dependency, GpuInfo, SystemMemoryInfo};
use gglib_db::{CoreFactory, setup_test_database};

// ---------------------------------------------------------------------------
// MockProcessRunner
// ---------------------------------------------------------------------------

/// A no-op `ProcessRunner` that always reports no running processes.
#[allow(dead_code)]
pub(crate) struct MockProcessRunner;

#[async_trait]
impl ProcessRunner for MockProcessRunner {
    async fn start(&self, _config: ServerConfig) -> Result<ProcessHandle, ProcessError> {
        Err(ProcessError::StartFailed(
            "mock: start not supported".to_string(),
        ))
    }

    async fn stop(&self, _handle: &ProcessHandle) -> Result<(), ProcessError> {
        Ok(())
    }

    async fn is_running(&self, _handle: &ProcessHandle) -> bool {
        false
    }

    async fn health(&self, _handle: &ProcessHandle) -> Result<ServerHealth, ProcessError> {
        Err(ProcessError::NotRunning("mock: no processes".to_string()))
    }

    async fn list_running(&self) -> Result<Vec<ProcessHandle>, ProcessError> {
        Ok(vec![])
    }
}

/// Convenience constructor for a no-op process runner.
#[allow(dead_code)]
pub(crate) fn noop_runner() -> Arc<MockProcessRunner> {
    Arc::new(MockProcessRunner)
}

// ---------------------------------------------------------------------------
// MockDownloadManager
// ---------------------------------------------------------------------------

/// A controllable stub for `DownloadManagerPort`.
///
/// - `fail_cancel = true` → `cancel_download` returns `DownloadError::NotFound`
/// - `reorder_position` → the position value returned by `reorder_queue`
pub(crate) struct MockDownloadManager {
    pub fail_cancel: bool,
    pub reorder_position: u32,
}

impl Default for MockDownloadManager {
    fn default() -> Self {
        Self {
            fail_cancel: false,
            reorder_position: 1,
        }
    }
}

impl MockDownloadManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// A variant whose `cancel_download` always returns `NotFound`.
    pub fn failing_cancel() -> Self {
        Self {
            fail_cancel: true,
            reorder_position: 1,
        }
    }
}

#[async_trait]
impl DownloadManagerPort for MockDownloadManager {
    async fn queue_download(&self, _request: DownloadRequest) -> Result<DownloadId, DownloadError> {
        Ok(DownloadId::from_model("mock-model"))
    }

    async fn queue_and_process(
        self: Arc<Self>,
        _request: DownloadRequest,
    ) -> Result<DownloadId, DownloadError> {
        Ok(DownloadId::from_model("mock-model"))
    }

    async fn queue_smart(
        self: Arc<Self>,
        _repo_id: String,
        _quantization: Option<String>,
    ) -> Result<(usize, usize), DownloadError> {
        Ok((1, 1))
    }

    async fn get_queue_snapshot(&self) -> Result<QueueSnapshot, DownloadError> {
        Ok(QueueSnapshot::default())
    }

    async fn cancel_download(&self, id: &DownloadId) -> Result<(), DownloadError> {
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

    async fn has_download(&self, _id: &DownloadId) -> Result<bool, DownloadError> {
        Ok(false)
    }

    async fn active_count(&self) -> Result<u32, DownloadError> {
        Ok(0)
    }

    async fn pending_count(&self) -> Result<u32, DownloadError> {
        Ok(0)
    }

    async fn remove_from_queue(&self, _id: &DownloadId) -> Result<(), DownloadError> {
        Ok(())
    }

    async fn reorder_queue(
        &self,
        _id: &DownloadId,
        _new_position: u32,
    ) -> Result<u32, DownloadError> {
        Ok(self.reorder_position)
    }

    async fn cancel_group(&self, _group_id: &str) -> Result<(), DownloadError> {
        Ok(())
    }

    async fn retry(&self, _id: &DownloadId) -> Result<u32, DownloadError> {
        Ok(1)
    }

    async fn clear_failed(&self) -> Result<(), DownloadError> {
        Ok(())
    }

    async fn set_max_queue_size(&self, _size: u32) -> Result<(), DownloadError> {
        Ok(())
    }

    async fn get_max_queue_size(&self) -> Result<u32, DownloadError> {
        Ok(10)
    }
}

// ---------------------------------------------------------------------------
// MockHfClient
// ---------------------------------------------------------------------------

/// Stub `HfClientPort` that returns minimal valid responses.
pub(crate) struct MockHfClient;

#[async_trait]
impl HfClientPort for MockHfClient {
    async fn search(&self, _options: &HfSearchOptions) -> Result<HfSearchResult, HfPortError> {
        Ok(HfSearchResult {
            items: vec![],
            has_more: false,
            page: 0,
        })
    }

    async fn list_quantizations(&self, _model_id: &str) -> Result<Vec<HfQuantInfo>, HfPortError> {
        Ok(vec![HfQuantInfo {
            name: "Q4_K_M".to_string(),
            shard_count: 1,
            total_size: 0,
            file_paths: vec!["model.gguf".to_string()],
        }])
    }

    async fn list_gguf_files(&self, _model_id: &str) -> Result<Vec<HfFileInfo>, HfPortError> {
        Ok(vec![])
    }

    async fn get_quantization_files(
        &self,
        _model_id: &str,
        _quantization: &str,
    ) -> Result<Vec<HfFileInfo>, HfPortError> {
        Ok(vec![])
    }

    async fn get_commit_sha(&self, _model_id: &str) -> Result<String, HfPortError> {
        Ok("abc123".to_string())
    }

    async fn get_model_info(&self, model_id: &str) -> Result<HfRepoInfo, HfPortError> {
        Ok(HfRepoInfo {
            model_id: model_id.to_string(),
            name: model_id.to_string(),
            author: None,
            downloads: 0,
            likes: 0,
            parameters_b: None,
            description: None,
            last_modified: None,
            chat_template: None,
            tags: vec![],
        })
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
            is_apple_silicon: false,
            has_nvidia_gpu: false,
        }
    }
}

// ---------------------------------------------------------------------------
// AppCore test helper
// ---------------------------------------------------------------------------

/// Build an `AppCore` backed by an in-memory SQLite database.
///
/// Uses the `test-utils` feature gate from `gglib-db`.
pub(crate) async fn test_core() -> Arc<AppCore> {
    let pool = setup_test_database().await.expect("in-memory DB");
    Arc::new(CoreFactory::build_app_core(
        pool,
        Arc::new(MockProcessRunner),
    ))
}
