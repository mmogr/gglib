//! What `gglib model download` says an image model's download brings beside
//! its weights, before it is queued.
//!
//! The preview is `DownloadOps::get_model_quantizations`', the listing the
//! web page's preview shows: one head read of the repository's weights says
//! whether it is an image model, and for one names each companion, whether
//! it is already in its repository's folder of the models directory, and the
//! bytes a download would fetch. What came of the companions, linked or
//! refused, is said at the end in the download's own words, which name
//! them.

use std::fmt::Write as _;
use std::path::PathBuf;
use std::sync::Arc;

use gglib_app_services::types::HfImagePreview;
use gglib_app_services::{DownloadDeps, DownloadOps};
use gglib_core::download::format_size;
use gglib_core::ports::{ToolSupportDetection, ToolSupportDetectionInput, ToolSupportDetectorPort};

use crate::bootstrap::CliContext;

/// The listing's operations over this command's Hub client, parser and
/// download manager. `models_directory` is where a companion is looked for;
/// `None` resolves it as the daemon does when it downloads.
pub(super) fn listing_ops(ctx: &CliContext, models_directory: Option<PathBuf>) -> DownloadOps {
    DownloadOps::new(DownloadDeps {
        downloads: Arc::clone(&ctx.downloads),
        hf: Arc::clone(&ctx.hf_client),
        tool_detector: Arc::new(NotAsked),
        gguf_parser: Arc::clone(&ctx.gguf_parser),
        models_directory,
    })
}

/// What to print before `model_id` is queued: the companions its download
/// brings when it is an image model, `None` for any other repository.
///
/// A listing that fails says nothing here. The preview only informs; the
/// download is queued all the same, and the daemon says why it fails if it
/// does.
pub(super) async fn preview(ops: &DownloadOps, model_id: &str) -> Option<String> {
    match ops.get_model_quantizations(model_id).await {
        Ok(listed) => listed.image.as_ref().map(preview_text),
        Err(unlisted) => {
            tracing::debug!(model_id, error = %unlisted, "no companion preview");
            None
        }
    }
}

/// The family, each companion with its repository, path and size, those
/// already here marked so, and the bytes left to fetch.
pub(super) fn preview_text(preview: &HfImagePreview) -> String {
    let role_width = (preview.companions.iter())
        .map(|companion| companion.role.label().len())
        .max()
        .unwrap_or(0);
    let mut text = format!(
        "{} image model: its download brings {} companion file(s) beside the weights.\n",
        preview.family.label(),
        preview.companions.len()
    );
    for companion in &preview.companions {
        let here = if companion.present {
            ", already here"
        } else {
            ""
        };
        let _ = writeln!(
            text,
            "  {:<role_width$}  {}/{} ({}{here})",
            companion.role.label(),
            companion.repo,
            companion.file_path,
            format_size(companion.size_bytes),
        );
    }
    if preview.fetch_bytes == 0 {
        text.push_str("  Every companion is already here; only the weights are fetched.\n");
    } else {
        let _ = writeln!(
            text,
            "  To fetch beside the weights: {}",
            format_size(preview.fetch_bytes)
        );
    }
    text
}

/// The listing's operations are built with a tool detector, which the
/// quantization listing never asks.
struct NotAsked;

impl ToolSupportDetectorPort for NotAsked {
    fn detect(&self, _input: ToolSupportDetectionInput<'_>) -> ToolSupportDetection {
        ToolSupportDetection {
            supports_tool_calling: false,
            confidence: 0.0,
            detected_format: None,
        }
    }
}

#[cfg(test)]
#[path = "companions_tests.rs"]
mod tests;
