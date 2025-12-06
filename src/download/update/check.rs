//! Update checking for downloaded models.

use anyhow::Result;
use std::sync::Arc;

use crate::commands::download::{create_hf_api, get_models_directory};
use crate::models::Gguf;
use crate::services::AppCore;

/// Check if a single model needs updates.
///
/// Compares the stored commit SHA with the latest SHA on HuggingFace.
pub async fn check_model_update(model: &Gguf, hf_repo: &str) -> Result<()> {
    println!("Checking updates for: {}", model.name);

    let models_dir = get_models_directory()?;
    let api = create_hf_api(None, &models_dir)?;
    let repo = api.repo(hf_hub::Repo::with_revision(
        hf_repo.to_string(),
        hf_hub::RepoType::Model,
        "main".to_string(),
    ));

    match repo.info() {
        Ok(repo_info) => {
            let latest_sha = repo_info.sha;

            if let Some(stored_sha) = &model.hf_commit_sha {
                if *stored_sha == latest_sha {
                    println!("  ✓ Model is up to date (SHA: {})", &latest_sha[..8]);
                } else {
                    println!("  🔄 Update available!");
                    println!("    Current SHA: {}", &stored_sha[..8]);
                    println!("    Latest SHA:  {}", &latest_sha[..8]);
                    println!(
                        "    Use: gglib update-model {} to update",
                        model.id.unwrap_or(0)
                    );
                }
            } else {
                println!("  ⚠️  No commit SHA stored, cannot check for updates");
            }
        }
        Err(e) => {
            println!("  ❌ Failed to check repository: {}", e);
        }
    }

    Ok(())
}

/// Handle the check-updates CLI command.
///
/// Can check a single model by ID or all models at once.
pub async fn handle_check_updates(
    core: Arc<AppCore>,
    model_id: Option<u32>,
    all: bool,
) -> Result<()> {
    if all {
        println!("Checking updates for all models...");
        let models = core.models().list().await?;

        if models.is_empty() {
            println!("No models found in database.");
            return Ok(());
        }

        for model in models {
            if let Some(hf_repo) = &model.hf_repo_id {
                check_model_update(&model, hf_repo).await?;
            } else {
                println!(
                    "Model '{}' is not from HuggingFace, skipping update check.",
                    model.name
                );
            }
        }
    } else if let Some(id) = model_id {
        match core.models().get_by_id(id).await {
            Ok(model) => {
                if let Some(hf_repo) = &model.hf_repo_id {
                    check_model_update(&model, hf_repo).await?;
                } else {
                    println!(
                        "Model '{}' is not from HuggingFace, cannot check for updates.",
                        model.name
                    );
                }
            }
            Err(_) => {
                println!("Model with ID {} not found.", id);
            }
        }
    } else {
        println!("Please specify --model-id <ID> or --all to check for updates.");
    }

    Ok(())
}
