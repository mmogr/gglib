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
        // Fetch more models than requested since we filter out models without GGUF files
        let fetch_query = HfSearchOptions {
            limit: 100,
            ..query.clone()
        };

        let url = build_search_url(&self.config, &fetch_query);
        let (hits, has_more): (Vec<serde_json::Value>, bool) =
            self.backend.get_json_paginated(&url).await?;

        // Apply parameter filtering (client-side)
        let items = search_hits(&hits)
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
            .take(query.limit as usize)
            .collect();

        Ok(HfSearchResult {
            items,
            has_more,
            page: query.page,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::tests::{fake_model_json, test_config};
    use crate::http::testing::{CannedResponse, FakeBackend};
    use serde_json::json;

    #[tokio::test]
    async fn test_search_models_page() {
        let backend = FakeBackend::new().with_response(
            "huggingface.co",
            CannedResponse {
                json: json!([
                    fake_model_json("Org/Model1-GGUF", 1000),
                    fake_model_json("Org/Model2-GGUF", 2000),
                ]),
                has_more: true,
            },
        );

        let client = HfClient::with_backend(test_config(), backend);
        let query = HfSearchOptions::new().with_query("llama");

        let response = client.search_models_page(&query).await.unwrap();

        assert_eq!(response.items.len(), 2);
        assert!(response.has_more);
        assert_eq!(response.items[0].model_id, "Org/Model1-GGUF");
    }

    #[tokio::test]
    async fn test_search_models_page_filters_by_params() {
        let backend = FakeBackend::new().with_response(
            "huggingface.co",
            CannedResponse {
                json: json!([
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
                ]),
                has_more: false,
            },
        );

        let client = HfClient::with_backend(test_config(), backend);

        // Filter for models between 5B and 100B params
        let query = HfSearchOptions::new().with_params_filter(Some(5.0), Some(100.0));

        let response = client.search_models_page(&query).await.unwrap();

        assert_eq!(response.items.len(), 1);
        assert_eq!(response.items[0].model_id, "Org/Large-GGUF");
    }
}
