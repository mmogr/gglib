//! `HuggingFace` file resolution.
//!
//! This module resolves quantization-specific files from `HuggingFace` repositories
//! using the `HfClientPort` abstraction.

use std::sync::Arc;

use async_trait::async_trait;

use gglib_core::download::{DownloadError, Quantization};
use gglib_core::ports::{HfClientPort, QuantizationResolver, Resolution, ResolvedFile};

/// Resolver that uses the `HuggingFace` client port.
pub struct HfQuantizationResolver {
    hf_client: Arc<dyn HfClientPort>,
}

impl HfQuantizationResolver {
    /// Create a new resolver with the given HF client.
    pub fn new(hf_client: Arc<dyn HfClientPort>) -> Self {
        Self { hf_client }
    }
}

#[async_trait]
impl QuantizationResolver for HfQuantizationResolver {
    async fn resolve(
        &self,
        repo_id: &str,
        quantization: Quantization,
    ) -> Result<Resolution, DownloadError> {
        // Get files for this quantization from HF client
        let quant_str = quantization.to_string();
        let files = self
            .hf_client
            .get_quantization_files(repo_id, &quant_str)
            .await
            .map_err(|e| {
                DownloadError::resolution_failed(format!("Failed to get quantization files: {e}"))
            })?;

        if files.is_empty() {
            return Err(DownloadError::resolution_failed(format!(
                "No files found for quantization {quantization} in {repo_id}"
            )));
        }

        // Check if this is a sharded model (multiple parts)
        let is_sharded =
            files.len() > 1 || files.iter().any(|f| f.path.contains("-00001-of-"));

        let resolved_files: Vec<_> = files
            .into_iter()
            .map(|file_info| ResolvedFile::with_size_and_oid(file_info.path, file_info.size, file_info.oid))
            .collect();

        Ok(Resolution {
            quantization,
            files: resolved_files,
            is_sharded,
        })
    }

    async fn list_available(&self, repo_id: &str) -> Result<Vec<Quantization>, DownloadError> {
        let quant_infos = self
            .hf_client
            .list_quantizations(repo_id)
            .await
            .map_err(|e| {
                DownloadError::resolution_failed(format!("Failed to list quantizations: {e}"))
            })?;

        // Convert HfQuantInfo to Quantization, filtering out Unknown
        let quantizations: Vec<_> = quant_infos
            .into_iter()
            .map(|info| Quantization::from_filename(&info.name))
            .filter(|q| !q.is_unknown())
            .collect();

        Ok(quantizations)
    }
}

#[cfg(test)]
mod tests {
    // TODO: Add tests with mock HfClientPort
}
