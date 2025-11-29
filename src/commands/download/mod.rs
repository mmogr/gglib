#![doc = include_str!(concat!(env!("OUT_DIR"), "/commands_download_docs.md"))]

use anyhow::Result;
use tokio_util::sync::CancellationToken;

// Sub-modules
mod api;
mod file_ops;
mod model_ops;
mod progress;
mod python_bridge;
mod utils;

// Re-export public functions
pub use api::*;
pub use file_ops::*;
pub use model_ops::*;
pub use progress::*;
pub(crate) use python_bridge::ensure_fast_helper_ready;
pub use utils::*;

/// Execute the download command
#[allow(clippy::too_many_arguments)]
pub async fn execute(
    model_id: String,
    quantization: Option<String>,
    list_quants: bool,
    add_to_db: bool,
    token: Option<String>,
    force: bool,
    progress_callback: Option<&ProgressCallback>,
    cancel_token: Option<CancellationToken>,
) -> Result<()> {
    // Get or create the models directory
    let models_dir = get_models_directory()?;

    // Initialize HuggingFace Hub API
    let api = create_hf_api(token.clone(), &models_dir)?;

    if list_quants {
        return list_available_quantizations(&api, &model_id).await;
    }

    let context = DownloadContext {
        model_id: &model_id,
        quantization: quantization.as_deref(),
        models_dir: &models_dir,
        force,
        add_to_db,
        session: SessionOptions {
            auth_token: token,
            progress_callback,
            cancel_token,
        },
    };

    // Download the model
    download_model(&api, context).await
}
