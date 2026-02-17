//! Repository file listing and quantization functionality.

use crate::error::{HfError, HfResult};
use crate::http::HttpBackend;
use crate::models::{HfFileEntry, HfQuantization, HfRepoRef};
use crate::parsing::{aggregate_quantizations, filter_files_by_quantization, parse_tree_entries};
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

    /// List available quantizations for a model.
    ///
    /// Scans the repository for GGUF files and groups them by quantization type.
    pub(crate) async fn list_quantizations(
        &self,
        repo: &HfRepoRef,
    ) -> HfResult<Vec<HfQuantization>> {
        let files = self.list_all_gguf_files(repo).await?;
        Ok(aggregate_quantizations(&files))
    }

    /// Find GGUF files for a specific quantization.
    pub(crate) async fn find_quantization_files(
        &self,
        repo: &HfRepoRef,
        quantization: &str,
    ) -> HfResult<Vec<HfFileEntry>> {
        let files = self.list_all_gguf_files(repo).await?;
        let matching = filter_files_by_quantization(&files, quantization);

        if matching.is_empty() {
            return Err(HfError::QuantizationNotFound {
                model_id: repo.id(),
                quantization: quantization.to_string(),
            });
        }

        Ok(matching)
    }

    /// Find GGUF files for a specific quantization, returning file entries with OIDs.
    pub(crate) async fn find_quantization_files_with_sizes(
        &self,
        repo: &HfRepoRef,
        quantization: &str,
    ) -> HfResult<Vec<HfFileEntry>> {
        let mut files = self.find_quantization_files(repo, quantization).await?;

        // Sort by path to ensure correct shard order
        files.sort_by(|a, b| a.path.cmp(&b.path));

        Ok(files)
    }

    /// Fetch model info (commit SHA, metadata, etc.).
    pub(crate) async fn get_model_info(&self, repo: &HfRepoRef) -> HfResult<serde_json::Value> {
        let url = build_model_info_url(&self.config, repo);
        self.backend.get_json(&url).await
    }

    /// Get the commit SHA for a model repository.
    pub(crate) async fn get_commit_sha(&self, repo: &HfRepoRef) -> HfResult<String> {
        let info = self.get_model_info(repo).await?;
        Ok(info
            .get("sha")
            .and_then(|v| v.as_str())
            .unwrap_or("main")
            .to_string())
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
            CannedResponse {
                json: json!([
                    {"path": "README.md", "type": "file", "size": 1000},
                    {"path": "model.Q4_K_M.gguf", "type": "file", "size": 4_000_000_000_u64},
                    {"path": "Q8_0", "type": "directory", "size": 0}
                ]),
                has_more: false,
            },
        );

        let client = HfClient::with_backend(test_config(), backend);
        let repo = HfRepoRef::new("TheBloke", "Llama-2-7B-GGUF");

        let files = client.list_model_files(&repo, None).await.unwrap();

        assert_eq!(files.len(), 3);
        assert!(files[1].is_gguf());
        assert!(files[2].is_directory());
    }

    #[tokio::test]
    async fn test_list_quantizations() {
        let backend = FakeBackend::new().with_response(
            "tree/main",
            CannedResponse {
                json: json!([
                    {"path": "model-Q4_K_M.gguf", "type": "file", "size": 4_000_000_000_u64},
                    {"path": "model-Q8_0.gguf", "type": "file", "size": 8_000_000_000_u64},
                ]),
                has_more: false,
            },
        );

        let client = HfClient::with_backend(test_config(), backend);
        let repo = HfRepoRef::new("TheBloke", "Llama-2-7B-GGUF");

        let quants = client.list_quantizations(&repo).await.unwrap();

        assert_eq!(quants.len(), 2);
        // Sorted alphabetically
        assert_eq!(quants[0].name, "Q4_K_M");
        assert_eq!(quants[1].name, "Q8_0");
    }

    #[tokio::test]
    async fn test_find_quantization_files() {
        let backend = FakeBackend::new().with_response(
            "tree/main",
            CannedResponse {
                json: json!([
                    {"path": "model-Q4_K_M.gguf", "type": "file", "size": 4_000_000_000_u64},
                    {"path": "model-Q8_0.gguf", "type": "file", "size": 8_000_000_000_u64},
                ]),
                has_more: false,
            },
        );

        let client = HfClient::with_backend(test_config(), backend);
        let repo = HfRepoRef::new("TheBloke", "Llama-2-7B-GGUF");

        let files = client
            .find_quantization_files(&repo, "Q4_K_M")
            .await
            .unwrap();

        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, "model-Q4_K_M.gguf");
    }

    #[tokio::test]
    async fn test_find_quantization_files_not_found() {
        let backend = FakeBackend::new().with_response(
            "tree/main",
            CannedResponse {
                json: json!([
                    {"path": "model-Q4_K_M.gguf", "type": "file", "size": 4_000_000_000_u64},
                ]),
                has_more: false,
            },
        );

        let client = HfClient::with_backend(test_config(), backend);
        let repo = HfRepoRef::new("TheBloke", "Llama-2-7B-GGUF");

        let result = client.find_quantization_files(&repo, "Q99_Z").await;

        assert!(matches!(result, Err(HfError::QuantizationNotFound { .. })));
    }

    #[tokio::test]
    async fn test_get_commit_sha() {
        let backend = FakeBackend::new().with_response(
            "Llama-2-7B-GGUF",
            CannedResponse {
                json: json!({
                    "id": "TheBloke/Llama-2-7B-GGUF",
                    "sha": "abc123def456"
                }),
                has_more: false,
            },
        );

        let client = HfClient::with_backend(test_config(), backend);
        let repo = HfRepoRef::new("TheBloke", "Llama-2-7B-GGUF");

        let sha = client.get_commit_sha(&repo).await.unwrap();

        assert_eq!(sha, "abc123def456");
    }

    #[tokio::test]
    async fn test_get_commit_sha_missing_defaults_to_main() {
        let backend = FakeBackend::new().with_response(
            "Llama-2-7B-GGUF",
            CannedResponse {
                json: json!({"id": "TheBloke/Llama-2-7B-GGUF"}),
                has_more: false,
            },
        );

        let client = HfClient::with_backend(test_config(), backend);
        let repo = HfRepoRef::new("TheBloke", "Llama-2-7B-GGUF");

        let sha = client.get_commit_sha(&repo).await.unwrap();

        assert_eq!(sha, "main");
    }
}
