//! A Hub read Hugging Face's rate limit refused, as the browser's calls and
//! the queue answer it: [`GuiError::HubLimited`], with the allowance and the
//! time until it lifts, and never a plain failure or a listing read without
//! its heads.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use gglib_core::download::DownloadError;
use gglib_core::hub_limit::HubBucket;
use gglib_core::ports::huggingface::fake_hub::{FakeHub, RoleParser, hub_file};
use gglib_core::ports::{HfFileInfo, HfPortError, HfQuantInfo, HfRepoInfo, HfSearchResult};

use super::*;
use crate::test_support::MockDownloadManager;

/// The time [`LimitedHub`]'s limit lifts in: 2:14.
const LIFTS_IN: Duration = Duration::from_secs(134);

fn limited() -> HfPortError {
    HfPortError::RateLimited {
        bucket: HubBucket::Api,
        resets_in: LIFTS_IN,
    }
}

/// A Hub whose API allowance is spent: it refuses every request.
struct LimitedHub;

#[async_trait]
impl HfClientPort for LimitedHub {
    async fn search(&self, _: &HfSearchOptions) -> Result<HfSearchResult, HfPortError> {
        Err(limited())
    }
    async fn list_quantizations(&self, _: &str) -> Result<Vec<HfQuantInfo>, HfPortError> {
        Err(limited())
    }
    async fn list_gguf_files(&self, _: &str) -> Result<Vec<HfFileInfo>, HfPortError> {
        Err(limited())
    }
    async fn get_quantization_files(
        &self,
        _: &str,
        _: &str,
    ) -> Result<Vec<HfFileInfo>, HfPortError> {
        Err(limited())
    }
    async fn get_commit_sha(&self, _: &str) -> Result<String, HfPortError> {
        Err(limited())
    }
    async fn get_model_info(&self, _: &str) -> Result<HfRepoInfo, HfPortError> {
        Err(limited())
    }
    async fn read_head(&self, _: &str, _: &str, _: u64) -> Result<Vec<u8>, HfPortError> {
        Err(limited())
    }
    async fn file_at(&self, _: &str, _: &str) -> Result<Option<HfFileInfo>, HfPortError> {
        Err(limited())
    }
}

/// The browser's calls over `hub`, with a queue that is never reached.
fn ops_over(hub: Arc<dyn HfClientPort>) -> DownloadOps {
    DownloadOps::new(DownloadDeps {
        downloads: Arc::new(MockDownloadManager::new()),
        hf: hub,
        gguf_parser: Arc::new(RoleParser),
        models_directory: None,
        core: None,
    })
}

/// The limit `refused` carries, and its sentence, or a panic naming what
/// it was instead.
fn hub_limited<T: std::fmt::Debug>(refused: Result<T, GuiError>) -> (HubBucket, Duration, String) {
    match refused {
        Err(GuiError::HubLimited {
            bucket,
            retry_after,
            message,
        }) => (bucket, retry_after, message),
        other => panic!("expected the Hub's rate limit, got {other:?}"),
    }
}

/// Each of the browser's Hub reads answers the limit as the limit, led by
/// what it was doing, its other failures' words kept.
#[tokio::test]
async fn every_browser_read_answers_the_hubs_limit_as_the_limit() {
    let ops = ops_over(Arc::new(LimitedHub));
    let said = |doing: &str| format!("{doing}: Hugging Face's rate limit lifts in 2:14");

    let searched = hub_limited(ops.search_hf_models(HfSearchRequest::default()).await);
    let listed = hub_limited(ops.get_model_quantizations("owner/repo").await);
    let picked = hub_limited(ops.get_model_pick("owner/repo").await);
    let summed = hub_limited(ops.get_model_summary("owner/repo").await);

    assert_eq!(
        [searched, listed, picked, summed],
        [
            (HubBucket::Api, LIFTS_IN, said("HF search failed")),
            (
                HubBucket::Api,
                LIFTS_IN,
                said("Failed to get quantizations")
            ),
            (HubBucket::Api, LIFTS_IN, said("Failed to pick a variant")),
            (HubBucket::Api, LIFTS_IN, said("Failed to get model info")),
        ]
    );
}

/// A repository of two sizes whose heads the Hub's file allowance refuses.
fn heads_refused() -> FakeHub {
    FakeHub {
        weights: vec![
            hub_file("m-Q4_K_M.gguf", 4 << 30, "a"),
            hub_file("m-Q8_0.gguf", 7 << 30, "b"),
        ],
        heads_limited: true,
        ..FakeHub::default()
    }
}

/// A listing whose heads the limit refused is the limit, not a listing of
/// sizes taken for plain models.
#[tokio::test]
async fn a_listing_whose_heads_were_refused_is_the_limit() {
    let ops = ops_over(Arc::new(heads_refused()));

    let (bucket, _, message) = hub_limited(ops.get_model_quantizations("owner/repo").await);

    assert_eq!(bucket, HubBucket::Resolvers);
    assert_eq!(
        message,
        "Failed to get quantizations: Hugging Face's rate limit lifts in 2:00"
    );
}

/// A download the Hub's rate limit kept the manager from choosing is the
/// limit, led by the queueing; the manager's other refusals keep their
/// words. (The manager's own answer is the bootstrap's test.)
#[tokio::test]
async fn a_queue_the_hub_refuses_is_the_limit() {
    let refusing = |refused| {
        DownloadOps::new(DownloadDeps {
            downloads: Arc::new(MockDownloadManager {
                queue_refusal: Some(refused),
                ..MockDownloadManager::default()
            }),
            hf: Arc::new(LimitedHub),
            gguf_parser: Arc::new(RoleParser),
            models_directory: None,
            core: None,
        })
    };
    let limited = DownloadError::from_hub(limited(), |_| unreachable!("a rate limit"));

    let queued = refusing(limited)
        .queue_download("owner/repo".to_owned(), Some("Q8_0".to_owned()))
        .await;
    let unresolved = refusing(DownloadError::resolution_failed("no such variant"))
        .queue_download("owner/repo".to_owned(), Some("Q8_0".to_owned()))
        .await;

    assert_eq!(
        hub_limited(queued),
        (
            HubBucket::Api,
            LIFTS_IN,
            "Could not queue the download: Hugging Face's rate limit lifts in 2:14".to_owned()
        )
    );
    assert!(
        matches!(&unresolved, Err(GuiError::Internal(said))
            if said == "Resolution failed: no such variant"),
        "{unresolved:?}"
    );
}
