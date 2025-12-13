//! Search handler for HuggingFace Hub.
//!
//! This command doesn't require AppCore - it's pure HF API calls.

use anyhow::{Result, anyhow};
use gglib_core::ports::huggingface::HfClientPort;
use gglib_hf::{DefaultHfClient, HfClientConfig};

/// Execute the search command.
///
/// Searches HuggingFace Hub for models matching the query.
/// No database access required.
pub async fn execute(query: String, limit: u32, sort: String, gguf_only: bool) -> Result<()> {
    println!("🔍 Searching HuggingFace Hub for: '{}'...", query);

    let client = DefaultHfClient::new(&HfClientConfig::default());

    // Default to GGUF filtering unless explicitly disabled
    let filter_gguf = gguf_only;

    // Build search options
    let options = gglib_core::ports::huggingface::HfSearchOptions {
        query: Some(query.clone()),
        limit: if filter_gguf { limit * 3 } else { limit },
        page: 0,
        sort_by: sort.clone(),
        sort_ascending: false,
        min_params_b: None,
        max_params_b: None,
    };

    // Use the service to fetch models
    let response = client
        .search(&options)
        .await
        .map_err(|e| anyhow!("Search failed: {}", e))?;

    let mut filtered_models = Vec::new();

    // Filter models based on gguf_only flag
    for model in &response.items {
        let model_id = &model.model_id;
        if filter_gguf {
            // Check if model actually has GGUF files
            if let Ok(quantizations) = client.list_quantizations(model_id).await {
                let names: Vec<String> = quantizations.iter().map(|q| q.name.clone()).collect();
                if !names.is_empty() {
                    filtered_models.push((model, names));
                }
            }
        } else {
            // Just check for any potential GGUF indicators in the name
            if model_id.to_lowercase().contains("gguf")
                || model_id.to_lowercase().contains("llama")
                || model_id.to_lowercase().contains("mistral")
                || model_id.to_lowercase().contains("qwen")
            {
                // Quick check for GGUF files
                if let Ok(quantizations) = client.list_quantizations(model_id).await {
                    let names: Vec<String> = quantizations.iter().map(|q| q.name.clone()).collect();
                    filtered_models.push((model, names));
                } else {
                    filtered_models.push((model, Vec::new()));
                }
            }
        }

        // Limit results
        if filtered_models.len() >= limit as usize {
            break;
        }
    }

    if filtered_models.is_empty() {
        if filter_gguf {
            println!("No GGUF models found for query: '{}'", query);
            println!(
                "💡 Try using a more general search term like 'gguf', 'llama-gguf', or specific model names"
            );
        } else {
            println!("No models found for query: '{}'", query);
        }
        return Ok(());
    }

    let model_type = if filter_gguf { "GGUF " } else { "" };
    println!("\n📋 Found {} {}models:", filtered_models.len(), model_type);
    println!("{}", "─".repeat(80));

    for (i, (model, quantizations)) in filtered_models.iter().enumerate() {
        println!(
            " {}. {} (↓{} ❤{})",
            i + 1,
            model.model_id,
            format_number(model.downloads),
            model.likes
        );

        // Show available quantizations for GGUF models
        if !quantizations.is_empty() {
            println!("    Quantizations: {}", quantizations.join(", "));
        }

        if let Some(ref desc) = model.description
            && !desc.is_empty()
        {
            let short_desc = if desc.len() > 80 {
                format!("{}...", &desc[..77])
            } else {
                desc.to_string()
            };
            println!("    {}", short_desc);
        }

        println!();
    }

    println!("💡 To download a model: gglib download <model_id>");
    println!("💡 To list quantizations: gglib download <model_id> --list-quants");

    Ok(())
}

/// Format large numbers with K/M suffixes.
fn format_number(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}K", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_number() {
        assert_eq!(format_number(500), "500");
        assert_eq!(format_number(1_500), "1.5K");
        assert_eq!(format_number(1_500_000), "1.5M");
    }
}
