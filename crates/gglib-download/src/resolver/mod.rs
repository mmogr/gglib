#![doc = include_str!("README.md")]
use std::sync::Arc;

use async_trait::async_trait;

use gglib_core::download::{DownloadError, GgufFileRole};
use gglib_core::ports::huggingface::{
    download_group, image_companions, label_refusal, match_variant, variant_role,
};
use gglib_core::ports::{
    GgufParserPort, HfClientPort, HfQuantInfo, QuantizationResolver, Resolution, ResolvedFile,
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
    /// The variant is the one `label` names as a request does, and the
    /// resolution carries its label as the listing gives it.
    async fn resolve(&self, repo_id: &str, label: &str) -> Result<Resolution, DownloadError> {
        let variants = self.list_available(repo_id).await?;
        let labels: Vec<&str> = variants.iter().map(|v| v.label.as_str()).collect();
        let label = match_variant(&variants, label)
            .map_err(|mismatch| {
                DownloadError::resolution_failed(label_refusal(repo_id, label, &labels, &mismatch))
            })?
            .label
            .as_str();

        // The weights of this variant, and the projector fetched with them
        // when the repository has one
        let group = download_group(self.hf_client.as_ref(), repo_id, label)
            .await
            .map_err(|e| {
                DownloadError::resolution_failed(format!("Failed to get quantization files: {e}"))
            })?;

        if group.weights.is_empty() {
            return Err(DownloadError::resolution_failed(format!(
                "No files found for variant {label} in {repo_id}"
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
            label: label.to_owned(),
            files: weights.chain(projector).chain(companions).collect(),
            is_sharded,
            image_family,
        })
    }

    async fn list_available(&self, repo_id: &str) -> Result<Vec<HfQuantInfo>, DownloadError> {
        self.hf_client
            .list_quantizations(repo_id)
            .await
            .map_err(|e| DownloadError::resolution_failed(format!("Failed to list variants: {e}")))
    }

    /// Read from the head of the variant's first file when the resolver has
    /// a parser; one that reads no head takes every variant for a model.
    async fn role_of(&self, repo_id: &str, variant: &HfQuantInfo) -> GgufFileRole {
        match &self.head_reader {
            Some(parser) => {
                variant_role(self.hf_client.as_ref(), parser.as_ref(), repo_id, variant).await
            }
            None => GgufFileRole::Weights,
        }
    }
}

#[cfg(test)]
mod tests;
