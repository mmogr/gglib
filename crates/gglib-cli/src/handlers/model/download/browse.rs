//! Browse handler for `HuggingFace` Hub.
//!
//! This command doesn't require `AppCore` - it's pure HF API calls.

use anyhow::{Result, anyhow};
use gglib_core::ports::huggingface::HfClientPort;
use gglib_hf::{DefaultHfClient, HfClientConfig};

use crate::presentation::{format_number, truncate_with};

/// Execute the browse command.
///
/// Browses popular/recent/trending GGUF models on `HuggingFace` Hub.
/// No database access required.
#[allow(
    clippy::match_same_arms,
    clippy::option_if_let_else,
    reason = "grandfathered at lint inheritance, #1157"
)]
pub(crate) async fn execute(category: String, limit: u32, size: Option<String>) -> Result<()> {
    let sort_param = match category.as_str() {
        "popular" => "downloads",
        "recent" => "created",
        "trending" => "trending",
        _ => "downloads",
    };

    println!("🌐 Browsing {category} GGUF models...");

    let client = DefaultHfClient::new(&HfClientConfig::default());

    // Search for models with GGUF-related tags
    let search_query = if let Some(ref model_size) = size {
        format!("gguf {model_size}")
    } else {
        "gguf".to_string()
    };

    // Build search options
    let options = gglib_core::ports::huggingface::HfSearchOptions {
        query: Some(search_query),
        limit,
        page: 0,
        sort_by: sort_param.to_string(),
        sort_ascending: false,
        min_params_b: None,
        max_params_b: None,
    };

    // Use the service to fetch models
    let response = client
        .search(&options)
        .await
        .map_err(|e| anyhow!("Search failed: {e}"))?;

    if response.items.is_empty() {
        println!("No {category} models found.");
        return Ok(());
    }

    println!("\n🏆 {} GGUF Models:", category.to_uppercase());
    println!("{}", "─".repeat(80));

    for (i, model) in response.items.iter().enumerate() {
        println!(
            "{:2}. {} (↓{} ❤{})",
            i + 1,
            model.model_id,
            format_number(model.downloads),
            model.likes
        );

        // Show available quantizations
        if let Ok(quantizations) = client.list_quantizations(&model.model_id).await {
            let names: Vec<&str> = quantizations.iter().map(|q| q.name.as_str()).collect();
            if !names.is_empty() {
                println!("    Quantizations: {}", names.join(", "));
            }
        }

        if let Some(ref desc) = model.description
            && !desc.is_empty()
        {
            println!("    {}", truncate_with(desc, 100, "..."));
        }

        println!();
    }

    println!("💡 To download a model: gglib model download <model_id>");
    println!("💡 To see all quantizations: gglib model download <model_id> --list-quants");

    Ok(())
}
