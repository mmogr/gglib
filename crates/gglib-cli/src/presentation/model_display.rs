//! Model summary display utilities for CLI output.

use gglib_core::Model;

/// Options for displaying a model summary.
#[derive(Debug, Clone, Default)]
pub(crate) struct ModelSummaryOpts<'a> {
    /// Optional title to display before the model details.
    pub title: Option<&'a str>,
    /// Whether to include the model ID.
    pub show_id: bool,
    /// Whether to include the file path.
    pub show_file_path: bool,
    /// Whether to include the added timestamp.
    pub show_added_at: bool,
}

impl<'a> ModelSummaryOpts<'a> {
    /// Create options with a title.
    pub(crate) fn with_title(title: &'a str) -> Self {
        Self {
            title: Some(title),
            show_file_path: true,
            ..Default::default()
        }
    }

    /// Create options for removal confirmation (includes ID and timestamp).
    pub(crate) fn for_removal() -> Self {
        Self {
            title: Some("Model to remove:"),
            show_id: true,
            show_file_path: true,
            show_added_at: true,
        }
    }
}

/// Display a model summary to stdout.
///
/// # Examples
///
/// ```rust,ignore
/// use crate::presentation::{display_model_summary, ModelSummaryOpts};
///
/// // Simple usage with title
/// display_model_summary(&model, &ModelSummaryOpts::with_title("Model created:"));
///
/// // For removal confirmation
/// display_model_summary(&model, &ModelSummaryOpts::for_removal());
/// ```
pub(crate) fn display_model_summary(model: &Model, opts: &ModelSummaryOpts) {
    for line in summary_lines(model, opts) {
        println!("{line}");
    }
}

/// The lines [`display_model_summary`] prints, in order.
///
/// A model whose parameter count is unknown has no `Parameters` line, as a
/// model lacking any other fact has none for it.
pub(crate) fn summary_lines(model: &Model, opts: &ModelSummaryOpts) -> Vec<String> {
    let mut lines = Vec::new();
    if let Some(title) = opts.title {
        lines.push(title.to_owned());
    }

    if opts.show_id {
        lines.push(format!("  ID: {}", model.id));
    }

    lines.push(format!("  Name: {}", model.name));

    if opts.show_file_path {
        lines.push(format!("  File: {}", model.file_path.display()));
    }

    // An unknown count is left out, as every other fact the model lacks is.
    if let Some(params) = model.param_count_b {
        lines.push(format!("  Parameters: {params:.1}B"));
    }

    if let Some(arch) = &model.architecture {
        lines.push(format!("  Architecture: {arch}"));
    }

    match (model.quant_label(), model.bits_per_weight) {
        (Some(quant), Some(bits)) => lines.push(format!("  Quantization: {quant} · {bits:.2} bpw")),
        (Some(quant), None) => lines.push(format!("  Quantization: {quant}")),
        (None, Some(bits)) => lines.push(format!("  Quantization: {bits:.2} bpw")),
        (None, None) => {}
    }

    if let Some(ctx) = model.context_length {
        lines.push(format!("  Context Length: {ctx} tokens"));
    }

    if opts.show_added_at {
        lines.push(format!(
            "  Added: {}",
            model.added_at.format("%Y-%m-%d %H:%M:%S UTC")
        ));
    }
    lines
}
