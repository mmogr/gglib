//! Update application for downloaded models.

use std::io::Write;
use std::sync::Arc;

use anyhow::Result;

use crate::commands::download::{
    FastDownloadRequest, get_models_directory, run_fast_download, sanitize_model_name,
};
use crate::download::DownloadResult;
use crate::download::domain::types::Quantization;
use crate::download::huggingface::QuantizationFileResolver;
use crate::download::workflows::register_model;
use crate::services::AppCore;

/// Handle the update-model CLI command.
///
/// Re-downloads a model to get the latest version from HuggingFace.
pub async fn handle_update_model(core: Arc<AppCore>, model_id: u32, force: bool) -> Result<()> {
    let model = match core.models().get_by_id(model_id).await {
        Ok(m) => m,
        Err(_) => {
            println!("Model with ID {} not found.", model_id);
            return Ok(());
        }
    };

    let hf_repo = match &model.hf_repo_id {
        Some(repo) => repo.clone(),
        None => {
            println!("Cannot update model: not downloaded from HuggingFace");
            return Ok(());
        }
    };

    let quantization_str = match &model.quantization {
        Some(q) => q.clone(),
        None => {
            println!("Cannot update model: no quantization information stored");
            return Ok(());
        }
    };

    println!("Updating model: {}", model.name);

    if !force {
        print!("This will re-download the model. Continue? [y/N]: ");
        std::io::stdout().flush()?;
        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        if !input.trim().to_lowercase().starts_with('y') {
            println!("Update cancelled.");
            return Ok(());
        }
    }

    // Get models directory
    let models_dir = get_models_directory()?;

    // Parse quantization from string
    let quantization = Quantization::from_filename(&quantization_str);

    // Resolve files using the HuggingFace resolver
    let resolver = QuantizationFileResolver::new();
    let resolution = resolver
        .resolve(&hf_repo, &quantization.to_string())
        .await
        .map_err(|e| anyhow::anyhow!("Failed to resolve files: {}", e))?;

    // Download the files
    let files: Vec<String> = resolution.filenames();
    let model_dir = models_dir.join(sanitize_model_name(&hf_repo));

    if !model_dir.exists() {
        std::fs::create_dir_all(&model_dir)?;
    }

    let fast_request = FastDownloadRequest {
        repo_id: &hf_repo,
        revision: "main",
        repo_type: "model",
        destination: &model_dir,
        files: &files,
        token: None,
        force: true,
        progress: None,
        cancel_token: None,
        pid_storage: None,
        pid_key: None,
    };

    run_fast_download(&fast_request).await?;

    // Register updated model
    let primary_path = model_dir.join(&files[0]);
    let result = DownloadResult {
        primary_path: primary_path.clone(),
        all_paths: files.iter().map(|f| model_dir.join(f)).collect(),
        quantization,
        repo_id: hf_repo.clone(),
        commit_sha: "main".to_string(), // TODO: Get actual commit SHA
        is_sharded: resolution.is_sharded,
        total_bytes: 0,
    };

    register_model(core, &result).await?;

    println!("✓ Model updated successfully!");

    Ok(())
}
