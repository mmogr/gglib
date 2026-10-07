//! The Hub search and the lookup by ID: what the Hub is asked, and the
//! summary the browser is answered with.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use gglib_core::ports::{
    HfFileInfo, HfPortError, HfQuantInfo, HfRepoInfo, HfSearchResult, HfSortField,
};

use super::*;
use crate::error::GuiError;
use crate::test_support::{MockDownloadManager, MockHfClient, MockToolSupportDetector};

/// A Hub that answers every search with one page of `hits`, and keeps what
/// each search asked for. A `limited` one refuses every search.
#[derive(Default)]
struct SearchedHub {
    hits: Vec<HfRepoInfo>,
    limited: bool,
    asked: Mutex<Vec<HfSearchOptions>>,
}

impl SearchedHub {
    /// What each search made so far asked for, oldest first.
    fn asked(&self) -> Vec<HfSearchOptions> {
        self.asked.lock().unwrap().clone()
    }
}

#[async_trait]
impl HfClientPort for SearchedHub {
    async fn search(&self, options: &HfSearchOptions) -> Result<HfSearchResult, HfPortError> {
        self.asked.lock().unwrap().push(options.clone());
        if self.limited {
            return Err(HfPortError::RateLimited);
        }
        Ok(HfSearchResult {
            items: self.hits.clone(),
            has_more: true,
            page: options.page,
        })
    }
    async fn get_model_info(&self, model_id: &str) -> Result<HfRepoInfo, HfPortError> {
        let hit = self.hits.iter().find(|hit| hit.model_id == model_id);
        hit.cloned().ok_or_else(|| HfPortError::ModelNotFound {
            model_id: model_id.to_string(),
        })
    }
    async fn list_quantizations(&self, model_id: &str) -> Result<Vec<HfQuantInfo>, HfPortError> {
        MockHfClient.list_quantizations(model_id).await
    }
    async fn list_projectors(&self, _: &str) -> Result<Vec<HfFileInfo>, HfPortError> {
        unimplemented!("a search and a lookup are all this hub answers")
    }
    async fn list_gguf_files(&self, _: &str) -> Result<Vec<HfFileInfo>, HfPortError> {
        unimplemented!("a search and a lookup are all this hub answers")
    }
    async fn get_quantization_files(
        &self,
        _: &str,
        _: &str,
    ) -> Result<Vec<HfFileInfo>, HfPortError> {
        unimplemented!("a search and a lookup are all this hub answers")
    }
    async fn get_commit_sha(&self, _: &str) -> Result<String, HfPortError> {
        unimplemented!("a search and a lookup are all this hub answers")
    }
}

/// The second hit of the search recorded in `gglib-hf`'s `search_fixture.json`,
/// as `gglib-hf` reads it, with a chat template the browser does not show.
fn recorded_hit() -> HfRepoInfo {
    HfRepoInfo {
        model_id: "unsloth/Phi-4-mini-instruct-GGUF".to_string(),
        name: "Phi-4-mini-instruct-GGUF".to_string(),
        author: Some("unsloth".to_string()),
        downloads: 117_114,
        likes: 153,
        parameters_b: Some(3.836_021_856),
        description: None,
        last_modified: None,
        chat_template: Some("{% for message in messages %}".to_string()),
        tags: vec!["gguf".to_string(), "phi3".to_string()],
    }
}

fn ops_over(hub: Arc<SearchedHub>) -> DownloadOps {
    DownloadOps::new(DownloadDeps {
        downloads: Arc::new(MockDownloadManager::new()),
        hf: hub,
        tool_detector: Arc::new(MockToolSupportDetector),
    })
}

/// The Hub is asked for what the request names, in every order: the order
/// is the request's own, and none is asked for as another.
#[tokio::test]
async fn a_search_asks_the_hub_for_exactly_what_the_request_names() {
    for sort_by in [
        HfSortField::Downloads,
        HfSortField::Likes,
        HfSortField::Modified,
        HfSortField::Created,
        HfSortField::Alphabetical,
    ] {
        let hub = SearchedHub::default();
        let request = HfSearchRequest {
            query: Some("phi-4 mini".to_string()),
            min_params_b: Some(1.5),
            max_params_b: Some(8.0),
            page: 2,
            limit: 7,
            sort_by,
            sort_ascending: true,
        };

        let found = search_hf_models(&hub, request).await.expect("a page");

        assert_eq!(
            (found.page, found.has_more, found.total_count),
            (2, true, None)
        );
        let asked = hub.asked();
        let [options] = asked.as_slice() else {
            panic!("one search was made: {asked:?}");
        };
        assert_eq!(options.query.as_deref(), Some("phi-4 mini"));
        assert_eq!(
            (options.min_params_b, options.max_params_b),
            (Some(1.5), Some(8.0))
        );
        assert_eq!((options.page, options.limit), (2, 7));
        assert_eq!(options.sort_by, sort_by);
        assert!(options.sort_ascending);
    }
}

/// The summary the browser is sent: the hit's fields under the wire's names,
/// and no chat template.
const RECORDED_HIT_ON_THE_WIRE: &str = r#"{
    "id": "unsloth/Phi-4-mini-instruct-GGUF",
    "name": "Phi-4-mini-instruct-GGUF",
    "author": "unsloth",
    "downloads": 117114,
    "likes": 153,
    "last_modified": null,
    "parameters_b": 3.836021856,
    "description": null,
    "tags": ["gguf", "phi3"]
}"#;

/// The browser's search is the one function the CLI calls, over the
/// handler's own Hub client, and each hit is answered as the summary the
/// browser shows.
#[tokio::test]
async fn the_browsers_search_is_the_shared_search_over_its_own_hub() {
    let hub = Arc::new(SearchedHub {
        hits: vec![recorded_hit()],
        ..SearchedHub::default()
    });
    let ops = ops_over(Arc::clone(&hub));
    let request = || HfSearchRequest {
        query: Some("phi-4 mini".to_string()),
        limit: 7,
        sort_by: HfSortField::Likes,
        ..HfSearchRequest::default()
    };

    let from_the_browser = ops.search_hf_models(request()).await.expect("a page");
    let from_the_cli = search_hf_models(hub.as_ref(), request())
        .await
        .expect("a page");

    let wire = |found| serde_json::to_value(found).unwrap();
    let expected: serde_json::Value = serde_json::from_str(RECORDED_HIT_ON_THE_WIRE).unwrap();
    assert_eq!(
        wire(&from_the_browser),
        serde_json::json!({
            "models": [expected],
            "has_more": true,
            "page": 0,
            "total_count": null,
        })
    );
    assert_eq!(wire(&from_the_browser), wire(&from_the_cli));
    // Both asked the Hub for the request, and so for the same thing.
    let asked: Vec<_> = hub
        .asked()
        .into_iter()
        .map(|o| (o.query, o.limit, o.sort_by))
        .collect();
    let named = (Some("phi-4 mini".to_string()), 7, HfSortField::Likes);
    assert_eq!(asked, [named.clone(), named]);
}

/// A repository looked up by its ID is answered as the summary a search
/// answers it as.
#[tokio::test]
async fn a_repository_looked_up_by_its_id_is_the_summary_a_search_gives() {
    let hub = Arc::new(SearchedHub {
        hits: vec![recorded_hit()],
        ..SearchedHub::default()
    });
    let ops = ops_over(hub);

    let looked_up = ops
        .get_model_summary("unsloth/Phi-4-mini-instruct-GGUF")
        .await
        .expect("a summary");

    let expected: serde_json::Value = serde_json::from_str(RECORDED_HIT_ON_THE_WIRE).unwrap();
    assert_eq!(serde_json::to_value(&looked_up).unwrap(), expected);
}

/// A search the Hub refuses is reported with the Hub's reason.
#[tokio::test]
async fn a_search_the_hub_refuses_says_why() {
    let hub = SearchedHub {
        limited: true,
        ..SearchedHub::default()
    };

    let refused = search_hf_models(&hub, HfSearchRequest::default()).await;

    assert!(
        matches!(&refused, Err(GuiError::Internal(reason))
            if reason == "HF search failed: Rate limit exceeded, try again later"),
        "{refused:?}"
    );
}
