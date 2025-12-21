//! Model summary display utilities for CLI output.

use gglib_core::Model;

/// Style options for model display.
#[derive(Debug, Clone, Copy, Default)]
pub enum DisplayStyle {
    /// Full details with all available fields.
    #[default]
    Default,
    /// Compact single-line format.
    Compact,
}

/// Options for displaying a model summary.
#[derive(Debug, Clone, Default)]
pub struct ModelSummaryOpts<'a> {
    /// Optional title to display before the model details.
    pub title: Option<&'a str>,
    /// Display style to use.
    pub style: DisplayStyle,
    /// Whether to include the model ID.
    pub show_id: bool,
    /// Whether to include the file path.
    pub show_file_path: bool,
    /// Whether to include the added timestamp.
    pub show_added_at: bool,
}

impl<'a> ModelSummaryOpts<'a> {
    /// Create options with a title and default style.
    pub fn with_title(title: &'a str) -> Self {
        Self {
            title: Some(title),
            show_file_path: true,
            ..Default::default()
        }
    }

    /// Create options for removal confirmation (includes ID and timestamp).
    pub fn for_removal() -> Self {
        Self {
            title: Some("Model to remove:"),
            show_id: true,
            show_file_path: true,
            show_added_at: true,
            ..Default::default()
        }
    }
}

/// Display a model summary to stdout.
///
/// # Examples
///
/// ```rust,ignore
/// use gglib_cli::presentation::{display_model_summary, ModelSummaryOpts};
///
/// // Simple usage with title
/// display_model_summary(&model, ModelSummaryOpts::with_title("Model created:"));
///
/// // For removal confirmation
/// display_model_summary(&model, ModelSummaryOpts::for_removal());
/// ```
pub fn display_model_summary(model: &Model, opts: ModelSummaryOpts) {
    if let Some(title) = opts.title {
        println!("{title}");
    }

    if opts.show_id {
        println!("  ID: {}", model.id);
    }

    println!("  Name: {}", model.name);

    if opts.show_file_path {
        println!("  File: {}", model.file_path.display());
    }

    println!("  Parameters: {:.1}B", model.param_count_b);

    if let Some(arch) = &model.architecture {
        println!("  Architecture: {arch}");
    }

    if let Some(quant) = &model.quantization {
        println!("  Quantization: {quant}");
    }

    if let Some(ctx) = model.context_length {
        println!("  Context Length: {ctx} tokens");
    }

    if opts.show_added_at {
        println!(
            "  Added: {}",
            model.added_at.format("%Y-%m-%d %H:%M:%S UTC")
        );
    }
}
