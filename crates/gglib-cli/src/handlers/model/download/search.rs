//! Search handler for `HuggingFace` Hub.
//!
//! The search is [`search_hf_hits`], the one the GUI's browser runs, over
//! the Hub client this process was bootstrapped with, which holds its Hub
//! token. This module prints the hits; `browse` prints them under its own
//! headings. Either asks for models that chat, or with `--images` for image
//! models.

use std::fmt::Write as _;

use anyhow::{Result, anyhow};
use gglib_app_services::types::{HfModelSummary, HfSearchRequest};
use gglib_app_services::{GuiError, search_hf_hits};
use gglib_core::ports::huggingface::{said_as_a_quantization, variants_of};
use gglib_core::ports::{HfClientPort, HfModelKind, HfQuantInfo, HfSortField};

use crate::presentation::{format_number, truncate_with};

/// The kind of model `--images` asks the Hub for.
pub(crate) const fn hub_kind(images: bool) -> HfModelKind {
    if images {
        HfModelKind::Image
    } else {
        HfModelKind::Chat
    }
}

/// Execute the search command.
///
/// Searches `HuggingFace` Hub for models of `kind` matching the query.
/// No database access required.
pub(crate) async fn execute(
    hf: &dyn HfClientPort,
    query: String,
    limit: u32,
    sort: HfSortField,
    kind: HfModelKind,
) -> Result<()> {
    let what = match kind {
        HfModelKind::Chat => "",
        HfModelKind::Image => "image models matching ",
    };
    println!("🔍 Searching HuggingFace Hub for {what}'{query}'...");
    print!("{}", found_text(hf, &query, limit, sort, kind).await?);
    Ok(())
}

/// What a search prints once the Hub has answered.
async fn found_text(
    hf: &dyn HfClientPort,
    query: &str,
    limit: u32,
    sort: HfSortField,
    kind: HfModelKind,
) -> Result<String> {
    let hits = find(hf, Some(query.to_string()), limit, sort, kind).await?;
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

/// The GGUF repositories of `kind` on the Hub's first page of `limit` for
/// `query`, or for no words at all, most of `sort_by` first, each with its
/// quantizations (no projector or importance matrix among them). Fewer than `limit` when the Hub's page is shorter, or when
/// hits are dropped for holding no GGUF file or not being of `kind`; a
/// `limit` above 100 is asked as 100.
pub(super) async fn find(
    hf: &dyn HfClientPort,
    query: Option<String>,
    limit: u32,
    sort_by: HfSortField,
    kind: HfModelKind,
) -> Result<Vec<Hit>> {
    let request = HfSearchRequest {
        query,
        limit,
        sort_by,
        kind,
        ..HfSearchRequest::default()
    };
    let found = search_hf_hits(hf, request)
        .await
        .map_err(|refused| match refused {
            GuiError::Internal(reason) => anyhow!(reason),
            other => other.into(),
        })?;

    // Each hit's variants are read from the files its answer names, with no
    // request of their own.
    Ok(found
        .items
        .into_iter()
        .map(|info| {
            let variants = variants_of(info.file_paths.iter().map(|path| (path.as_str(), 0)));
            Hit {
                model: info.into(),
                quantizations: said_labels(variants),
            }
        })
        .collect())
}

/// The labels of `variants` said as quantisations beside a hit: a
/// projector's and an importance matrix's are left out, by their file names.
fn said_labels(variants: Vec<HfQuantInfo>) -> Vec<String> {
    variants
        .into_iter()
        .filter(said_as_a_quantization)
        .map(|variant| variant.label)
        .collect()
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
