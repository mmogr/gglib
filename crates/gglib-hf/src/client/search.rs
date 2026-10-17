//! Search functionality for the `HuggingFace` client.

use gglib_core::ports::huggingface::{HfSearchOptions, HfSearchResult};

use crate::error::HfResult;
use crate::http::HttpBackend;
use crate::parsing::search_hits;
use crate::url::build_search_url;

use super::HfClient;

impl<B: HttpBackend> HfClient<B> {
    /// Search for models with pagination.
    ///
    /// Returns a single page of results with pagination info.
    pub(crate) async fn search_models_page(
        &self,
        query: &HfSearchOptions,
    ) -> HfResult<HfSearchResult> {
        // Every hit of the Hub's page that passes the filters is returned:
        // the next page starts where this one ended, so a hit left out here
        // would never be shown.
        let url = build_search_url(&self.config, query);
        let (hits, next_cursor): (Vec<serde_json::Value>, Option<String>) =
            self.backend.get_json_paginated(&url).await?;

        // Apply parameter filtering (client-side)
        let items = search_hits(&hits, query.kind)
            .into_iter()
            .filter(|model| {
                // Min params filter
                if let Some(min) = query.min_params_b {
                    match model.parameters_b {
                        Some(params) if params >= min => {}
                        _ => return false,
                    }
                }

                // Max params filter
                if let Some(max) = query.max_params_b {
                    match model.parameters_b {
                        Some(params) if params <= max => {}
                        _ => return false,
                    }
                }

                true
            })
            .collect();

        Ok(HfSearchResult { items, next_cursor })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::tests::{fake_model_json, test_config};
    use crate::http::testing::{CannedResponse, FakeBackend};
    use serde_json::json;

    /// A search asks the Hub for the page it was asked for, returns every hit
    /// of it, and carries on from the cursor the Hub gave.
    #[tokio::test]
    async fn a_search_pages_by_the_hubs_cursor() {
        let backend = FakeBackend::new()
            .with_response(
                "limit=7",
                CannedResponse {
                    json: json!([
                        fake_model_json("Org/Model1-GGUF", 1000),
                        fake_model_json("Org/Model2-GGUF", 2000),
                    ]),
                    next_cursor: Some("page2".to_string()),
                },
            )
            .with_response(
                "cursor=page2",
                CannedResponse::of(json!([fake_model_json("Org/Model3-GGUF", 500)])),
            );
        let client = HfClient::with_backend(test_config(), backend);
        let ids = |found: &HfSearchResult| -> Vec<String> {
            found.items.iter().map(|m| m.model_id.clone()).collect()
        };

        let first = HfSearchOptions::new().with_query("llama").with_limit(7);
        let first = client.search_models_page(&first).await.unwrap();

        assert_eq!(ids(&first), ["Org/Model1-GGUF", "Org/Model2-GGUF"]);
        assert_eq!(first.next_cursor.as_deref(), Some("page2"));

        let next = HfSearchOptions::new()
            .with_query("llama")
            .with_limit(8)
            .with_cursor(first.next_cursor.unwrap());
        let next = client.search_models_page(&next).await.unwrap();

        assert_eq!(ids(&next), ["Org/Model3-GGUF"]);
        assert_eq!(next.next_cursor, None);
    }

    /// Every hit of the Hub's page that passes the filters comes back, even
    /// past the limit: a hit cut here would never be shown on any page.
    #[tokio::test]
    async fn a_search_cuts_no_hit_of_the_hubs_page() {
        let backend = FakeBackend::new().with_default(CannedResponse::of(json!([
            fake_model_json("Org/Model1-GGUF", 1000),
            fake_model_json("Org/Model2-GGUF", 2000),
        ])));
        let client = HfClient::with_backend(test_config(), backend);

        let query = HfSearchOptions::new().with_limit(1);
        let found = client.search_models_page(&query).await.unwrap();

        assert_eq!(found.items.len(), 2);
    }

    #[tokio::test]
    async fn test_search_models_page_filters_by_params() {
        let backend = FakeBackend::new().with_response(
            "huggingface.co",
            CannedResponse::of(json!([
                {
                    "id": "Org/Small-GGUF",
                    "downloads": 1000,
                    "siblings": [{"rfilename": "model.gguf"}],
                    "gguf": {"total": 1_000_000_000_u64}  // 1B params
                },
                {
                    "id": "Org/Large-GGUF",
                    "downloads": 2000,
                    "siblings": [{"rfilename": "model.gguf"}],
                    "gguf": {"total": 70_000_000_000_u64}  // 70B params
                },
            ])),
        );

        let client = HfClient::with_backend(test_config(), backend);

        // Filter for models between 5B and 100B params
        let query = HfSearchOptions::new().with_params_filter(Some(5.0), Some(100.0));

        let response = client.search_models_page(&query).await.unwrap();

        assert_eq!(response.items.len(), 1);
        assert_eq!(response.items[0].model_id, "Org/Large-GGUF");
    }
}
