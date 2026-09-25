//! `check_update_with` against a hand-written hub that answers only the
//! commit question.

use super::*;
use async_trait::async_trait;
use gglib_core::ports::huggingface::{
    HfFileInfo, HfPortError, HfPortResult, HfQuantInfo, HfRepoInfo, HfSearchOptions, HfSearchResult,
};

const REPO: &str = "owner/repo";
const RECORDED: &str = "1111111111111111111111111111111111111111";
const NEWER: &str = "2222222222222222222222222222222222222222";

/// A hub whose model info for [`REPO`] names `sha`, or names no commit when
/// `sha` is `None`. Any other repo is not found.
struct Hub {
    sha: Option<&'static str>,
}

#[async_trait]
impl HfClientPort for Hub {
    async fn get_commit_sha(&self, model_id: &str) -> HfPortResult<String> {
        if model_id != REPO {
            return Err(HfPortError::ModelNotFound {
                model_id: model_id.to_string(),
            });
        }
        self.sha
            .map(str::to_string)
            .ok_or_else(|| HfPortError::InvalidResponse {
                message: format!("the model info for {REPO} names no commit sha"),
            })
    }

    async fn search(&self, _options: &HfSearchOptions) -> HfPortResult<HfSearchResult> {
        unimplemented!("not reached by check_update_with")
    }
    async fn list_quantizations(&self, _model_id: &str) -> HfPortResult<Vec<HfQuantInfo>> {
        unimplemented!("not reached by check_update_with")
    }
    async fn list_gguf_files(&self, _model_id: &str) -> HfPortResult<Vec<HfFileInfo>> {
        unimplemented!("not reached by check_update_with")
    }
    async fn get_quantization_files(
        &self,
        _model_id: &str,
        _quantization: &str,
    ) -> HfPortResult<Vec<HfFileInfo>> {
        unimplemented!("not reached by check_update_with")
    }
    async fn get_model_info(&self, _model_id: &str) -> HfPortResult<HfRepoInfo> {
        unimplemented!("not reached by check_update_with")
    }
}

#[tokio::test]
async fn a_hub_that_names_the_recorded_sha_is_not_an_update() {
    let hub = Hub {
        sha: Some(RECORDED),
    };

    let check = check_update_with(&hub, REPO, Some(RECORDED)).await.unwrap();

    assert!(!check.has_update);
    assert_eq!(check.current_sha.as_deref(), Some(RECORDED));
    assert_eq!(check.latest_sha, RECORDED);
}

#[tokio::test]
async fn a_hub_past_the_recorded_sha_is_an_update() {
    let hub = Hub { sha: Some(NEWER) };

    let check = check_update_with(&hub, REPO, Some(RECORDED)).await.unwrap();

    assert!(check.has_update);
    assert_eq!(check.current_sha.as_deref(), Some(RECORDED));
    assert_eq!(check.latest_sha, NEWER);
}

#[tokio::test]
async fn a_model_with_no_recorded_sha_is_an_update() {
    let hub = Hub { sha: Some(NEWER) };

    let check = check_update_with(&hub, REPO, None).await.unwrap();

    assert!(check.has_update);
    assert_eq!(check.current_sha, None);
    assert_eq!(check.latest_sha, NEWER);
}

#[tokio::test]
async fn a_hub_that_names_no_sha_fails_the_check() {
    let hub = Hub { sha: None };

    let error = check_update_with(&hub, REPO, Some(RECORDED))
        .await
        .unwrap_err();

    assert!(error.to_string().contains("names no commit sha"), "{error}");
}
