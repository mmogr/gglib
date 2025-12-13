//! Search functionality for the `HuggingFace` client.

// Some search methods are for future use (CLI raw search, paginated all models)
#![allow(dead_code)]

use crate::error::HfResult;
use crate::http::HttpBackend;
use crate::models::{HfModelSummary, HfSearchQuery, HfSearchResponse, HfSortField};
use crate::parsing::parse_search_response;
use crate::url::build_search_url;

use super::HfClient;

impl<B: HttpBackend> HfClient<B> {
    /// Search for models with pagination.
    ///
    /// Returns a single page of results with pagination info.
    pub(crate) async fn search_models_page(
        &self,
        query: &HfSearchQuery,
    ) -> HfResult<HfSearchResponse> {
        // Fetch more models than requested since we filter out models without GGUF files
        let fetch_query = HfSearchQuery {
            limit: 100,
            ..query.clone()
        };

        let url = build_search_url(&self.config, &fetch_query);
        let (json_array, has_more): (Vec<serde_json::Value>, bool) =
            self.backend.get_json_paginated(&url).await?;

        let mut response = parse_search_response(&json_array, has_more, query.page);

        // Apply parameter filtering (client-side)
        response.items = response
            .items
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

        Ok(response)
    }

    /// Search for all models matching a query.
    ///
    /// Fetches all pages and returns a combined list.
    /// Use with caution for broad queries as this may make many API calls.
    pub(crate) async fn search_all_models(
        &self,
        query: HfSearchQuery,
    ) -> HfResult<Vec<HfModelSummary>> {
        let mut results = Vec::new();
        let mut current_query = HfSearchQuery { page: 0, ..query };

        loop {
            let response = self.search_models_page(&current_query).await?;
            results.extend(response.items);

            if !response.has_more {
                break;
            }

            current_query.page += 1;

            // Safety limit to prevent infinite loops
            if current_query.page > 100 {
                break;
            }
        }

        Ok(results)
    }

    /// Search for models (simple interface for CLI).
    ///
    /// Returns raw JSON values for flexible output formatting.
    pub(crate) async fn search_models_raw(
        &self,
        search_query: &str,
        limit: u32,
        sort: &str,
    ) -> HfResult<Vec<serde_json::Value>> {
        let sort_field = match sort {
            "likes" => HfSortField::Likes,
            "created" | "createdAt" => HfSortField::Created,
            "modified" | "lastModified" => HfSortField::Modified,
            "id" | "alphabetical" => HfSortField::Alphabetical,
            _ => HfSortField::Downloads,
        };

        let query = HfSearchQuery::new()
            .with_query(search_query)
            .with_limit(limit)
            .with_sort(sort_field, false);

        let url = build_search_url(&self.config, &query);
        let result: Vec<serde_json::Value> = self.backend.get_json(&url).await?;
        Ok(result)
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
        let query = HfSearchQuery::new().with_query("llama");

        let response = client.search_models_page(&query).await.unwrap();

        assert_eq!(response.items.len(), 2);
        assert!(response.has_more);
        assert_eq!(response.items[0].id, "Org/Model1-GGUF");
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
        let query = HfSearchQuery::new().with_params_filter(Some(5.0), Some(100.0));

        let response = client.search_models_page(&query).await.unwrap();

        assert_eq!(response.items.len(), 1);
        assert_eq!(response.items[0].id, "Org/Large-GGUF");
    }
}
