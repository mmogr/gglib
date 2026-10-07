//! Browse handler for `HuggingFace` Hub.
//!
//! A browse is a search with a fixed query and the category's order: it runs
//! [`search`](mod@super::search)'s search and prints its listing.

use anyhow::Result;
use gglib_core::ports::HfClientPort;

use crate::model_sort::CliBrowseCategory;

use super::search::{Layout, find, listing};

/// Execute the browse command.
///
/// Browses the popular or the recent GGUF models on `HuggingFace` Hub.
/// No database access required.
pub(crate) async fn execute(
    hf: &dyn HfClientPort,
    category: CliBrowseCategory,
    limit: u32,
    size: Option<String>,
) -> Result<()> {
    println!("🌐 Browsing {} GGUF models...", category.name());
    print!("{}", found_text(hf, category, limit, size).await?);
    Ok(())
}

/// What a browse prints once the Hub has answered.
async fn found_text(
    hf: &dyn HfClientPort,
    category: CliBrowseCategory,
    limit: u32,
    size: Option<String>,
) -> Result<String> {
    let query = size.map_or_else(|| "gguf".to_string(), |size| format!("gguf {size}"));
    let hits = find(hf, query, limit, category.into()).await?;
    if hits.is_empty() {
        return Ok(format!("No {} models found.\n", category.name()));
    }
    let heading = format!("🏆 {} GGUF Models:", category.name().to_uppercase());
    Ok(listing(&heading, &hits, &LAYOUT))
}

const LAYOUT: Layout = Layout {
    number: |n| format!("{n:2}. "),
    description_width: 100,
    quantizations_tip: "To see all quantizations",
};

#[cfg(test)]
#[path = "browse_tests.rs"]
mod tests;
