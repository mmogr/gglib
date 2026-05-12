//! [`DownloadTriggerAdapter`] — bridges `DownloadManagerPort` → `DownloadTriggerPort`.
//!
//! `ModelVerificationService` needs a `DownloadTriggerPort` to queue
//! downloads, but the download manager implements the richer
//! `DownloadManagerPort`. This adapter performs the conversion.
//!
//! Kept `pub(crate)` because it is consumed only by [`crate::builder::CoreBootstrap::build`].

use std::sync::Arc;

use async_trait::async_trait;
use gglib_core::download::DownloadError;
use gglib_core::ports::DownloadManagerPort;
use gglib_core::services::DownloadTriggerPort;

pub struct DownloadTriggerAdapter {
    pub download_manager: Arc<dyn DownloadManagerPort>,
}

#[async_trait]
impl DownloadTriggerPort for DownloadTriggerAdapter {
    async fn queue_download(
        &self,
        repo_id: String,
        quantization: Option<String>,
    ) -> anyhow::Result<String> {
        use gglib_core::download::Quantization;
        use gglib_core::ports::DownloadRequest;
        use std::str::FromStr;

        // Convert quantization string to enum; default to Q4_K_M when absent.
        let quant = quantization
            .as_ref()
            .and_then(|q| Quantization::from_str(q).ok())
            .unwrap_or(Quantization::Q4KM);

        let request = DownloadRequest::new(repo_id, quant);
        let id = self
            .download_manager
            .queue_download(request)
            .await
            .map_err(|e: DownloadError| anyhow::anyhow!("Failed to queue download: {e}"))?;

        Ok(id.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::sync::Mutex;

    use gglib_core::download::{DownloadError, DownloadId, Quantization, QueueSnapshot};
    use gglib_core::ports::{DownloadManagerPort, DownloadRequest};

    // ── Minimal mock ──────────────────────────────────────────────────────────

    struct MockDownloadManager {
        /// The last request received by `queue_download`.
        captured: Mutex<Option<DownloadRequest>>,
        /// When true, `queue_download` returns `DownloadError::QueueFull`.
        should_fail: bool,
    }

    impl MockDownloadManager {
        fn new() -> Arc<Self> {
            Arc::new(Self {
                captured: Mutex::new(None),
                should_fail: false,
            })
        }

        fn failing() -> Arc<Self> {
            Arc::new(Self {
                captured: Mutex::new(None),
                should_fail: true,
            })
        }

        fn captured_quantization(&self) -> Option<Quantization> {
            self.captured
                .lock()
                .unwrap()
                .as_ref()
                .map(|r| r.quantization.clone())
        }
    }

    #[async_trait]
    impl DownloadManagerPort for MockDownloadManager {
        async fn queue_download(
            &self,
            request: DownloadRequest,
        ) -> Result<DownloadId, DownloadError> {
            if self.should_fail {
                return Err(DownloadError::QueueFull { max_size: 0 });
            }
            let id = DownloadId::new(&request.repo_id, Some(request.quantization.to_string()));
            *self.captured.lock().unwrap() = Some(request);
            Ok(id)
        }

        async fn queue_and_process(
            self: Arc<Self>,
            _request: DownloadRequest,
        ) -> Result<DownloadId, DownloadError> {
            unimplemented!()
        }

        async fn queue_smart(
            self: Arc<Self>,
            _repo_id: String,
            _quantization: Option<String>,
        ) -> Result<(usize, usize), DownloadError> {
            unimplemented!()
        }

        async fn get_queue_snapshot(&self) -> Result<QueueSnapshot, DownloadError> {
            unimplemented!()
        }

        async fn cancel_download(&self, _id: &DownloadId) -> Result<(), DownloadError> {
            unimplemented!()
        }

        async fn cancel_all(&self) -> Result<(), DownloadError> {
            unimplemented!()
        }

        async fn has_download(&self, _id: &DownloadId) -> Result<bool, DownloadError> {
            unimplemented!()
        }

        async fn active_count(&self) -> Result<u32, DownloadError> {
            unimplemented!()
        }

        async fn pending_count(&self) -> Result<u32, DownloadError> {
            unimplemented!()
        }

        async fn remove_from_queue(&self, _id: &DownloadId) -> Result<(), DownloadError> {
            unimplemented!()
        }

        async fn reorder_queue(
            &self,
            _id: &DownloadId,
            _new_position: u32,
        ) -> Result<u32, DownloadError> {
            unimplemented!()
        }

        async fn cancel_group(&self, _group_id: &str) -> Result<(), DownloadError> {
            unimplemented!()
        }

        async fn retry(&self, _id: &DownloadId) -> Result<u32, DownloadError> {
            unimplemented!()
        }

        async fn clear_failed(&self) -> Result<(), DownloadError> {
            unimplemented!()
        }

        async fn set_max_queue_size(&self, _size: u32) -> Result<(), DownloadError> {
            unimplemented!()
        }

        async fn get_max_queue_size(&self) -> Result<u32, DownloadError> {
            unimplemented!()
        }
    }

    // ── Helpers ───────────────────────────────────────────────────────────────

    fn adapter_with(manager: Arc<MockDownloadManager>) -> DownloadTriggerAdapter {
        DownloadTriggerAdapter {
            download_manager: manager,
        }
    }

    // ── Tests ─────────────────────────────────────────────────────────────────

    /// When no quantization is provided the adapter must fall back to Q4_K_M.
    #[tokio::test]
    async fn none_quantization_defaults_to_q4km() {
        let mgr = MockDownloadManager::new();
        let adapter = adapter_with(mgr.clone());

        adapter
            .queue_download("owner/model".to_string(), None)
            .await
            .unwrap();

        assert_eq!(mgr.captured_quantization(), Some(Quantization::Q4KM));
    }

    /// A recognised quantization string is forwarded verbatim.
    #[tokio::test]
    async fn known_quantization_string_is_forwarded() {
        let mgr = MockDownloadManager::new();
        let adapter = adapter_with(mgr.clone());

        adapter
            .queue_download("owner/model".to_string(), Some("Q8_0".to_string()))
            .await
            .unwrap();

        assert_eq!(mgr.captured_quantization(), Some(Quantization::Q8_0));
    }

    /// An unrecognised quantization string must fall back to Q4_K_M.
    #[tokio::test]
    async fn unknown_quantization_string_falls_back_to_q4km() {
        let mgr = MockDownloadManager::new();
        let adapter = adapter_with(mgr.clone());

        adapter
            .queue_download("owner/model".to_string(), Some("BANANA".to_string()))
            .await
            .unwrap();

        assert_eq!(mgr.captured_quantization(), Some(Quantization::Q4KM));
    }

    /// An error from the download manager is propagated as an `anyhow` error.
    #[tokio::test]
    async fn download_manager_error_is_propagated() {
        let adapter = adapter_with(MockDownloadManager::failing());
        let result = adapter
            .queue_download("owner/model".to_string(), None)
            .await;
        assert!(result.is_err());
    }
}
