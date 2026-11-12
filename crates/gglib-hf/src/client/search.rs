//! Search functionality for the `HuggingFace` client.

use futures_util::{StreamExt, stream};
use gglib_core::ports::huggingface::{HfSearchOptions, HfSearchResult};

use crate::error::HfResult;
use crate::http::HttpBackend;
use crate::models::HfRepoRef;
use crate::parsing::{gguf_sibling_paths, listed_repo_info, names_contradict_the_hub, search_hits};
use crate::url::build_search_url;

use super::HfClient;

/// How many hits' files are listed at once.
const LISTINGS_AT_ONCE: usize = 8;

impl<B: HttpBackend> HfClient<B> {
    /// Search for models with pagination.
    ///
    /// Returns a single page of results with pagination info. A hit keeps
    /// the Hub's parameter count unless its model files' names contradict
    /// it ([`names_contradict_the_hub`]): only such a hit's GGUF files are
    /// sized, in one request each, eight hits at once, and its count is the
    /// one its largest weights could hold ([`listed_repo_info`]). A hit
    /// whose files cannot be sized keeps the Hub's count.
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

        let hits = search_hits(&hits, query.kind);
        // The GGUF files of each hit to size: none unless its names
        // contradict the Hub's count.
        let asked: Vec<Option<(HfRepoRef, Vec<String>)>> = hits
            .iter()
            .map(|(id, hit)| {
                let repo = HfRepoRef::parse(id).filter(|_| names_contradict_the_hub(hit))?;
                Some((repo, gguf_sibling_paths(hit)))
            })
            .collect();
        let listings: Vec<_> = stream::iter(asked)
            .map(|asked| async move {
                let (repo, paths) = asked?;
                self.sized_variants(&repo, &paths).await.ok()
            })
            .buffered(LISTINGS_AT_ONCE)
            .collect()
            .await;

        // The size filters judge the checked count, so a hit the Hub
        // miscounted is kept or left out by its own.
        let in_range = |parameters_b: Option<f64>| {
            let at_least = query
                .min_params_b
                .is_none_or(|min| parameters_b.is_some_and(|params| params >= min));
            let at_most = query
                .max_params_b
                .is_none_or(|max| parameters_b.is_some_and(|params| params <= max));
            at_least && at_most
        };
        let items = hits
            .into_iter()
            .zip(listings)
            .filter_map(|((_, hit), variants)| listed_repo_info(hit, variants.as_deref()))
            .filter(|model| in_range(model.parameters_b))
            .collect();

        Ok(HfSearchResult { items, next_cursor })
    }
}

#[cfg(test)]
#[path = "search_tests.rs"]
mod tests;
