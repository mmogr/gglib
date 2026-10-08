//! Search handler for `HuggingFace` Hub.
//!
//! The search is [`search_hf_models`], the one the GUI's browser runs, over
//! the Hub client this process was bootstrapped with, which holds its Hub
//! token. This module prints the hits; `browse` prints them under its own
//! headings.

use std::fmt::Write as _;

use anyhow::{Result, anyhow};
use gglib_app_services::types::{HfModelSummary, HfSearchRequest};
use gglib_app_services::{GuiError, search_hf_models};
use gglib_core::ports::{HfClientPort, HfSortField};

use crate::presentation::{format_number, truncate_with};

/// Execute the search command.
///
/// Searches `HuggingFace` Hub for models matching the query.
/// No database access required.
pub(crate) async fn execute(
    hf: &dyn HfClientPort,
    query: String,
    limit: u32,
    sort: HfSortField,
) -> Result<()> {
    println!("🔍 Searching HuggingFace Hub for: '{query}'...");
    print!("{}", found_text(hf, &query, limit, sort).await?);
    Ok(())
}

/// What a search prints once the Hub has answered.
async fn found_text(
    hf: &dyn HfClientPort,
    query: &str,
    limit: u32,
    sort: HfSortField,
) -> Result<String> {
    let hits = find(hf, query.to_string(), limit, sort).await?;
    if hits.is_empty() {
        return Ok(format!("No models found for query: '{query}'\n"));
    }
    let heading = format!("📋 Found {} models:", hits.len());
    Ok(listing(&heading, &hits, &LAYOUT))
}

const LAYOUT: Layout = Layout {
    number: |n| format!(" {n}. "),
    description_width: 80,
    quantizations_tip: "To list quantizations",
};

/// A hit, and the names of its quantizations to show beside it.
pub(super) struct Hit {
    model: HfModelSummary,
    quantizations: Vec<String>,
}

/// What `search` and `browse` lay out differently in a listing.
pub(super) struct Layout {
    /// A hit's number, as it starts the hit's line.
    pub number: fn(usize) -> String,
    /// The length a description is cut to.
    pub description_width: usize,
    /// How the closing tip names the listing of quantizations.
    pub quantizations_tip: &'static str,
}

/// The first `limit` GGUF repositories the Hub finds for `query`, most of
/// `sort_by` first, each with its quantizations.
pub(super) async fn find(
    hf: &dyn HfClientPort,
    query: String,
    limit: u32,
    sort_by: HfSortField,
) -> Result<Vec<Hit>> {
    let request = HfSearchRequest {
        query: Some(query),
        limit,
        sort_by,
        ..HfSearchRequest::default()
    };
    let found = search_hf_models(hf, request)
        .await
        .map_err(|refused| match refused {
            GuiError::Internal(reason) => anyhow!(reason),
            other => other.into(),
        })?;

    let mut hits = Vec::with_capacity(found.models.len());
    for model in found.models {
        // Shown beside the hit and no more than that: a repository whose
        // files cannot be listed is a hit all the same.
        let quantizations = hf.list_quantizations(&model.id).await.unwrap_or_default();
        hits.push(Hit {
            model,
            quantizations: quantizations.into_iter().map(|q| q.name).collect(),
        });
    }
    Ok(hits)
}

/// `hits` under `heading`, one numbered entry each, and the closing tips.
pub(super) fn listing(heading: &str, hits: &[Hit], layout: &Layout) -> String {
    let mut text = format!("\n{heading}\n{}\n", "─".repeat(80));
    for (
        i,
        Hit {
            model,
            quantizations,
        },
    ) in hits.iter().enumerate()
    {
        let _ = writeln!(
            text,
            "{}{} (↓{} ❤{})",
            (layout.number)(i + 1),
            model.id,
            format_number(model.downloads),
            model.likes
        );
        if !quantizations.is_empty() {
            let _ = writeln!(text, "    Quantizations: {}", quantizations.join(", "));
        }
        if let Some(description) = model.description.as_deref().filter(|d| !d.is_empty()) {
            let cut = truncate_with(description, layout.description_width, "...");
            let _ = writeln!(text, "    {cut}");
        }
        text.push('\n');
    }
    let _ = writeln!(
        text,
        "💡 To download a model: gglib model download <model_id>\n\
         💡 {}: gglib model download <model_id> --list-quants",
        layout.quantizations_tip
    );
    text
}

#[cfg(test)]
#[path = "search_tests.rs"]
mod tests;
