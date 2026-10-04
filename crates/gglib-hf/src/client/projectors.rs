//! Listing a repository's projectors.

use crate::error::HfResult;
use crate::file_roles::projectors_among;
use crate::http::HttpBackend;
use crate::models::{HfFileEntry, HfRepoRef};

use super::HfClient;

impl<B: HttpBackend> HfClient<B> {
    /// List the projector files in a repository, by path, with their OIDs.
    pub(crate) async fn list_projectors(&self, repo: &HfRepoRef) -> HfResult<Vec<HfFileEntry>> {
        let files = self.list_all_gguf_files(repo).await?;
        Ok(projectors_among(&files))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::tests::test_config;
    use crate::error::HfError;
    use crate::http::testing::{CannedResponse, FakeBackend};
    use serde_json::json;

    /// A repository whose root holds `Q8_0` weights, the projector of that
    /// quantization, and an `F16` projector.
    fn client() -> HfClient<FakeBackend> {
        let backend = FakeBackend::new().with_response(
            "tree/main",
            CannedResponse {
                json: json!([
                    {"path": "X.Q8_0.gguf", "type": "file", "size": 8_000, "lfs": {"oid": "w"}},
                    {"path": "X.mmproj-Q8_0.gguf", "type": "file", "size": 600, "lfs": {"oid": "p8"}},
                    {"path": "mmproj-F16.gguf", "type": "file", "size": 900, "lfs": {"oid": "pf"}},
                ]),
                has_more: false,
            },
        );
        HfClient::with_backend(test_config(), backend)
    }

    fn repo() -> HfRepoRef {
        HfRepoRef::new("owner", "X-GGUF")
    }

    #[tokio::test]
    async fn the_projectors_are_listed_with_their_oids() {
        let projectors = client().list_projectors(&repo()).await.unwrap();

        let listed: Vec<_> = projectors
            .iter()
            .map(|p| (p.path.as_str(), p.size, p.oid.as_deref()))
            .collect();
        assert_eq!(
            listed,
            [
                ("X.mmproj-Q8_0.gguf", 600, Some("p8")),
                ("mmproj-F16.gguf", 900, Some("pf"))
            ]
        );
    }

    #[tokio::test]
    async fn the_quantizations_are_the_weights_alone() {
        let quantizations = client().list_quantizations(&repo()).await.unwrap();

        assert_eq!(quantizations.len(), 1);
        assert_eq!(quantizations[0].name, "Q8_0");
        assert_eq!(quantizations[0].paths, ["X.Q8_0.gguf"]);
        assert_eq!(quantizations[0].shard_count, 1);
    }

    #[tokio::test]
    async fn the_files_of_a_quantization_are_the_weights_alone() {
        let files = client()
            .find_quantization_files_with_sizes(&repo(), "Q8_0")
            .await
            .unwrap();

        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, "X.Q8_0.gguf");
    }

    /// `F16` is the name of a projector here and of no weights.
    #[tokio::test]
    async fn a_quantization_only_a_projector_carries_is_not_found() {
        let refused = client().find_quantization_files(&repo(), "F16").await;

        assert!(matches!(refused, Err(HfError::QuantizationNotFound { .. })));
    }
}
