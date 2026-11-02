//! Repository file listing and quantization functionality.

use crate::error::{HfError, HfResult};
use crate::http::HttpBackend;
use gglib_core::ports::huggingface::HfQuantInfo;

use crate::models::{HfFileEntry, HfRepoRef};
use crate::parsing::{parse_tree_entries, variant_files, variants};
use crate::url::{build_model_info_url, build_tree_url};

use super::HfClient;

impl<B: HttpBackend> HfClient<B> {
    /// List files in a model repository.
    pub(crate) async fn list_model_files(
        &self,
        repo: &HfRepoRef,
        path: Option<&str>,
    ) -> HfResult<Vec<HfFileEntry>> {
        let url = build_tree_url(&self.config, repo, path);
        let json: serde_json::Value = self.backend.get_json(&url).await?;
        parse_tree_entries(&json)
    }

    /// List all GGUF files in a repository (including subdirectories).
    ///
    /// This recursively scans subdirectories to find all GGUF files,
    /// which is necessary for repositories that organize files by quantization.
    pub(crate) async fn list_all_gguf_files(&self, repo: &HfRepoRef) -> HfResult<Vec<HfFileEntry>> {
        let mut all_files = Vec::new();

        // Get root files
        let root_files = self.list_model_files(repo, None).await?;

        for file in &root_files {
            if file.is_gguf() {
                all_files.push(file.clone());
            } else if file.is_directory() {
                // Check subdirectory for GGUF files
                if let Ok(sub_files) = self.list_model_files(repo, Some(&file.path)).await {
                    for sub_file in sub_files {
                        if sub_file.is_gguf() {
                            all_files.push(sub_file);
                        }
                    }
                }
            }
        }

        Ok(all_files)
    }

    /// The repository's variants: its GGUF files grouped into shard sets,
    /// each labelled as its uploader named it. No header is read.
    pub(crate) async fn list_variants(&self, repo: &HfRepoRef) -> HfResult<Vec<HfQuantInfo>> {
        let files = self.list_all_gguf_files(repo).await?;
        Ok(variants(&files))
    }

    /// The files of the variant `label` names, in shard order, with their
    /// OIDs.
    pub(crate) async fn find_variant_files(
        &self,
        repo: &HfRepoRef,
        label: &str,
    ) -> HfResult<Vec<HfFileEntry>> {
        let files = self.list_all_gguf_files(repo).await?;
        variant_files(&files, label).ok_or_else(|| HfError::QuantizationNotFound {
            model_id: repo.id(),
            quantization: label.to_string(),
        })
    }

    /// Fetch model info (commit SHA, metadata, etc.).
    pub(crate) async fn get_model_info(&self, repo: &HfRepoRef) -> HfResult<serde_json::Value> {
        let url = build_model_info_url(&self.config, repo);
        self.backend.get_json(&url).await
    }

    /// Get the commit SHA for a model repository.
    ///
    /// Fails with `InvalidResponse` when the model info has no non-empty
    /// string `sha`; no branch name stands in for the commit.
    pub(crate) async fn get_commit_sha(&self, repo: &HfRepoRef) -> HfResult<String> {
        let info = self.get_model_info(repo).await?;
        info.get("sha")
            .and_then(serde_json::Value::as_str)
            .filter(|sha| !sha.is_empty())
            .map(str::to_string)
            .ok_or_else(|| HfError::InvalidResponse {
                message: format!("the model info for {} names no commit sha", repo.id()),
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::tests::test_config;
    use crate::http::testing::{CannedResponse, FakeBackend};
    use serde_json::json;

    #[tokio::test]
    async fn test_list_model_files() {
        let backend = FakeBackend::new().with_response(
            "tree/main",
            CannedResponse::of(json!([
                {"path": "README.md", "type": "file", "size": 1000},
                {"path": "model.Q4_K_M.gguf", "type": "file", "size": 4_000_000_000_u64},
                {"path": "Q8_0", "type": "directory", "size": 0}
            ])),
        );

        let client = HfClient::with_backend(test_config(), backend);
        let repo = HfRepoRef::new("TheBloke", "Llama-2-7B-GGUF");

        let files = client.list_model_files(&repo, None).await.unwrap();

        assert_eq!(files.len(), 3);
        assert!(files[1].is_gguf());
        assert!(files[2].is_directory());
    }

    #[tokio::test]
    async fn test_list_variants() {
        let backend = FakeBackend::new().with_response(
            "tree/main",
            CannedResponse::of(json!([
                {"path": "model-Q4_K_M.gguf", "type": "file", "size": 4_000_000_000_u64},
                {"path": "model-Q8_0.gguf", "type": "file", "size": 8_000_000_000_u64},
            ])),
        );

        let client = HfClient::with_backend(test_config(), backend);
        let repo = HfRepoRef::new("TheBloke", "Llama-2-7B-GGUF");

        let quants = client.list_variants(&repo).await.unwrap();

        assert_eq!(quants.len(), 2);
        // Sorted by label
        assert_eq!(quants[0].label, "Q4_K_M");
        assert_eq!(quants[1].label, "Q8_0");
    }

    #[tokio::test]
    async fn test_find_variant_files() {
        let backend = FakeBackend::new().with_response(
            "tree/main",
            CannedResponse::of(json!([
                {"path": "model-Q4_K_M.gguf", "type": "file", "size": 4_000_000_000_u64},
                {"path": "model-Q8_0.gguf", "type": "file", "size": 8_000_000_000_u64},
            ])),
        );

        let client = HfClient::with_backend(test_config(), backend);
        let repo = HfRepoRef::new("TheBloke", "Llama-2-7B-GGUF");

        let files = client.find_variant_files(&repo, "Q4_K_M").await.unwrap();

        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, "model-Q4_K_M.gguf");
    }

    #[tokio::test]
    async fn test_find_variant_files_not_found() {
        let backend = FakeBackend::new().with_response(
            "tree/main",
            CannedResponse::of(json!([
                {"path": "model-Q4_K_M.gguf", "type": "file", "size": 4_000_000_000_u64},
            ])),
        );

        let client = HfClient::with_backend(test_config(), backend);
        let repo = HfRepoRef::new("TheBloke", "Llama-2-7B-GGUF");

        let result = client.find_variant_files(&repo, "Q99_Z").await;

        assert!(matches!(result, Err(HfError::QuantizationNotFound { .. })));
    }

    #[tokio::test]
    async fn test_find_variant_files_excludes_unsloth_ud_variant() {
        // Regression: a request for the plain "Q6_K" quantization must not
        // also return the unrelated Unsloth Dynamic "UD-Q6_K" file, even
        // though both share the same "Q6_K" bit-depth suffix.
        let backend = FakeBackend::new().with_response(
            "tree/main",
            CannedResponse::of(json!([
                {"path": "model-Q6_K.gguf", "type": "file", "size": 6_000_000_000_u64},
                {"path": "model-UD-Q6_K.gguf", "type": "file", "size": 6_500_000_000_u64},
            ])),
        );

        let client = HfClient::with_backend(test_config(), backend);
        let repo = HfRepoRef::new("unsloth", "Qwen3-Coder-Next-GGUF");

        let plain = client.find_variant_files(&repo, "Q6_K").await.unwrap();
        assert_eq!(plain.len(), 1);
        assert_eq!(plain[0].path, "model-Q6_K.gguf");

        let dynamic = client.find_variant_files(&repo, "UD-Q6_K").await.unwrap();
        assert_eq!(dynamic.len(), 1);
        assert_eq!(dynamic[0].path, "model-UD-Q6_K.gguf");
    }

    #[tokio::test]
    async fn test_get_commit_sha() {
        let backend = FakeBackend::new().with_response(
            "Llama-2-7B-GGUF",
            CannedResponse::of(json!({
                "id": "TheBloke/Llama-2-7B-GGUF",
                "sha": "abc123def456"
            })),
        );

        let client = HfClient::with_backend(test_config(), backend);
        let repo = HfRepoRef::new("TheBloke", "Llama-2-7B-GGUF");

        let sha = client.get_commit_sha(&repo).await.unwrap();

        assert_eq!(sha, "abc123def456");
    }

    #[tokio::test]
    async fn a_model_info_that_names_no_commit_sha_is_an_invalid_response() {
        let with_sha = |sha| json!({"id": "TheBloke/Llama-2-7B-GGUF", "sha": sha});
        let infos = [
            json!({"id": "TheBloke/Llama-2-7B-GGUF"}),
            with_sha(json!(null)),
            with_sha(json!(42)),
            with_sha(json!("")),
        ];

        for info in infos {
            let backend = FakeBackend::new()
                .with_response("Llama-2-7B-GGUF", CannedResponse::of(info.clone()));
            let client = HfClient::with_backend(test_config(), backend);
            let repo = HfRepoRef::new("TheBloke", "Llama-2-7B-GGUF");

            let result = client.get_commit_sha(&repo).await;

            assert!(
                matches!(result, Err(HfError::InvalidResponse { .. })),
                "{info}: {result:?}"
            );
        }
    }
}
