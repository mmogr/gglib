#![doc = include_str!("README.md")]
use std::sync::Arc;

use async_trait::async_trait;

use gglib_core::download::{DownloadError, Quantization};
use gglib_core::ports::huggingface::{download_group, image_companions};
use gglib_core::ports::{
    GgufParserPort, HfClientPort, QuantizationResolver, Resolution, ResolvedFile,
};

/// Resolver that uses the `HuggingFace` client port.
pub(crate) struct HfQuantizationResolver {
    hf_client: Arc<dyn HfClientPort>,
    /// Reads the head of the weights, to know an image model and bring its
    /// companions. `None` resolves the repository's own files alone.
    head_reader: Option<Arc<dyn GgufParserPort>>,
}

impl HfQuantizationResolver {
    /// Create a new resolver with the given HF client, which reads the head
    /// of the weights with `parser` and brings an image model's companions.
    pub(crate) fn new(hf_client: Arc<dyn HfClientPort>, parser: Arc<dyn GgufParserPort>) -> Self {
        Self {
            hf_client,
            head_reader: Some(parser),
        }
    }

    /// Create a resolver that resolves the repository's own files alone,
    /// the weights and the projector, and reads no head.
    pub(crate) fn weights_only(hf_client: Arc<dyn HfClientPort>) -> Self {
        Self {
            hf_client,
            head_reader: None,
        }
    }
}

#[async_trait]
impl QuantizationResolver for HfQuantizationResolver {
    async fn resolve(
        &self,
        repo_id: &str,
        quantization: Quantization,
    ) -> Result<Resolution, DownloadError> {
        // The weights of this quantization, and the projector fetched with
        // them when the repository has one
        let group = download_group(self.hf_client.as_ref(), repo_id, quantization)
            .await
            .map_err(|e| {
                DownloadError::resolution_failed(format!("Failed to get quantization files: {e}"))
            })?;

        if group.weights.is_empty() {
            return Err(DownloadError::resolution_failed(format!(
                "No files found for quantization {quantization} in {repo_id}"
            )));
        }

        // Check if this is a sharded model (multiple parts). Only the weights
        // are shards.
        let is_sharded =
            group.weights.len() > 1 || group.weights.iter().any(|f| f.path.contains("-00001-of-"));

        // An image model's family, from the head of its first weights file,
        // and the companions its recipe draws with, each from its own
        // repository. Anything else has none.
        let image = match &self.head_reader {
            Some(parser) => image_companions(
                self.hf_client.as_ref(),
                parser.as_ref(),
                repo_id,
                &group.weights[0],
            )
            .await
            .map_err(|e| {
                DownloadError::resolution_failed(format!(
                    "Failed to find the image model's companions: {e}"
                ))
            })?,
            None => None,
        };
        let (image_family, companions) = image
            .map_or((None, Vec::new()), |(family, companions)| {
                (Some(family), companions)
            });

        // Weights first, so the group's first file is always a weights file;
        // then the projector, then the companions.
        let weights = group
            .weights
            .into_iter()
            .map(|file| ResolvedFile::with_size_and_oid(file.path, file.size, file.oid));
        let projector = group
            .projector
            .map(|file| ResolvedFile::projector(file.path, file.size, file.oid));
        let companions = companions.into_iter().map(|companion| {
            ResolvedFile::companion(
                companion.role,
                companion.repo,
                companion.file.path,
                companion.file.size,
                companion.file.oid,
            )
        });

        Ok(Resolution {
            quantization,
            files: weights.chain(projector).chain(companions).collect(),
            is_sharded,
            image_family,
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
mod tests;
