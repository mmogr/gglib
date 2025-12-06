//! Metadata parsing and model update operations.

use crate::models::Gguf;
use anyhow::{Result, anyhow};
use std::collections::HashMap;

use super::args::UpdateArgs;

/// Parse metadata updates from command line arguments
pub fn parse_metadata_updates(metadata_args: &[String]) -> Result<HashMap<String, String>> {
    let mut metadata = HashMap::new();

    for arg in metadata_args {
        if let Some((key, value)) = arg.split_once('=') {
            metadata.insert(key.to_string(), value.to_string());
        } else {
            return Err(anyhow!(
                "Invalid metadata format '{}'. Use 'key=value'",
                arg
            ));
        }
    }

    Ok(metadata)
}

/// Parse metadata keys to remove
pub fn parse_metadata_removals(remove_arg: &Option<String>) -> Result<Vec<String>> {
    match remove_arg {
        Some(keys_str) => Ok(keys_str.split(',').map(|s| s.trim().to_string()).collect()),
        None => Ok(Vec::new()),
    }
}

/// Create updated model with new values
pub fn create_updated_model(
    existing: &Gguf,
    args: &UpdateArgs,
    metadata_updates: &HashMap<String, String>,
    metadata_removals: &[String],
) -> Result<Gguf> {
    let mut updated = existing.clone();

    // Update basic fields
    if let Some(name) = &args.name {
        updated.name = name.clone();
    }
    if let Some(param_count) = args.param_count {
        updated.param_count_b = param_count;
    }
    if let Some(architecture) = &args.architecture {
        updated.architecture = Some(architecture.clone());
    }
    if let Some(quantization) = &args.quantization {
        updated.quantization = Some(quantization.clone());
    }
    if let Some(context_length) = args.context_length {
        updated.context_length = Some(context_length);
    }

    // Handle metadata updates
    if args.replace_metadata {
        // Replace entire metadata with new values
        updated.metadata = metadata_updates.clone();
    } else {
        // Merge metadata updates
        for (key, value) in metadata_updates {
            updated.metadata.insert(key.clone(), value.clone());
        }
    }

    // Remove specified metadata keys
    for key in metadata_removals {
        updated.metadata.remove(key);
    }

    Ok(updated)
}
