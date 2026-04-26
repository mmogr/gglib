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

pub(crate) struct DownloadTriggerAdapter {
    pub(crate) download_manager: Arc<dyn DownloadManagerPort>,
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
