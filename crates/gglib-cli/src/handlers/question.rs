//! Question command handler.
//!
//! Handles asking a question with optional piped stdin context.

use anyhow::{Context, Result, anyhow};
use std::io::{self, Read, IsTerminal};
use std::process::Stdio;

use crate::bootstrap::CliContext;
use gglib_core::paths::llama_cli_path;
use gglib_runtime::llama::{ContextResolution, LlamaCommandBuilder, resolve_context_size};

/// Execute the question command.
///
/// This command allows asking a question with or without piped context.
/// If stdin is piped, it will be used as context. The `{}` placeholder
/// in the question will be replaced with the piped input, or if no
/// placeholder exists, the input will be prepended to the question.
///
/// # Arguments
///
/// * `ctx` - The CLI context providing access to AppCore
/// * `question` - The question to ask
/// * `model` - Optional model identifier (ID or name)
/// * `ctx_size` - Optional context size override
/// * `mlock` - Whether to enable memory lock
///
/// # Returns
///
/// Returns `Result<()>` indicating success or failure.
pub async fn execute(
    ctx: &CliContext,
    question: String,
    model: Option<String>,
    ctx_size: Option<String>,
    mlock: bool,
) -> Result<()> {
    // Check if stdin is piped
    let stdin = io::stdin();
    let is_piped = !stdin.is_terminal();

    // Read piped input if available
    let piped_input = if is_piped {
        let mut buffer = String::new();
        stdin.lock().read_to_string(&mut buffer)
            .context("Failed to read from stdin")?;
        Some(buffer)
    } else {
        None
    };

    // Resolve the model: --model flag -> settings default -> error
    let model = resolve_model(ctx, model.as_deref()).await?;

    // Build the prompt based on whether we have piped input
    let prompt = build_prompt(&question, piped_input.as_deref())?;

    // Calculate intelligent context size
    let context_resolution = calculate_context_size(
        &prompt,
        ctx_size.as_deref(),
        model.context_length,
    )?;

    // Get llama-cli path
    let llama_cli_path = llama_cli_path()
        .context("Failed to resolve llama-cli path")?;

    // Build and execute the command
    // Use simple -p flag which works with chat-tuned models
    let mut cmd = LlamaCommandBuilder::new(&llama_cli_path, &model.file_path)
        .context_resolution(context_resolution)
        .mlock(mlock)
        .build();

    // Add prompt with single-turn mode (exit after response)
    cmd.arg("-p").arg(&prompt)
        .arg("--single-turn"); // exit after one response

    cmd.stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());

    let status = cmd.status()
        .context("Failed to execute llama-cli")?;

    if !status.success() {
        return Err(anyhow!("llama-cli exited with error"));
    }

    Ok(())
}

/// Resolve the model to use for the question.
///
/// Resolution order:
/// 1. Explicit --model flag
/// 2. Default model from settings
/// 3. Error with helpful message
async fn resolve_model(
    ctx: &CliContext,
    model_identifier: Option<&str>,
) -> Result<gglib_core::Model> {
    if let Some(identifier) = model_identifier {
        // User specified a model explicitly
        ctx.app()
            .models()
            .find_by_identifier(identifier)
            .await
            .context(format!("Failed to find model: {}", identifier))
    } else {
        // Try to use default model from settings
        let settings = ctx.app().settings().get().await
            .context("Failed to load settings")?;

        match settings.default_model_id {
            Some(model_id) => {
                ctx.app()
                    .models()
                    .get_by_id(model_id)
                    .await
                    .context("Failed to load default model")?
                    .ok_or_else(|| {
                        anyhow!(
                            "Default model (ID: {}) not found. \
                             Update default with: gglib config settings set-default-model <id-or-name>",
                            model_id
                        )
                    })
            }
            None => {
                Err(anyhow!(
                    "No model specified and no default model set.\n\
                     Use --model <id-or-name> or set a default:\n  \
                     gglib config settings set-default-model <id-or-name>"
                ))
            }
        }
    }
}

/// Build the prompt based on the question and optional piped input.
///
/// If piped input is provided:
/// - If question contains `{}`, replace it with the piped input
/// - Otherwise, prepend the piped input before the question
///
/// If no piped input, just use the question as-is.
fn build_prompt(question: &str, piped_input: Option<&str>) -> Result<String> {
    match piped_input {
        Some(input) if !input.is_empty() => {
            if question.contains("{}") {
                // Replace {} placeholder with piped input
                Ok(question.replace("{}", input))
            } else {
                // Prepend input as context
                Ok(format!(
                    "Context:\n{}\n\nQuestion: {}\n\nAnswer:",
                    input.trim(),
                    question
                ))
            }
        }
        _ => {
            // No piped input, use question directly
            Ok(question.to_string())
        }
    }
}

/// Calculate intelligent context size based on input length and model capabilities.
///
/// Estimates token count using chars/4 heuristic and validates against model max.
fn calculate_context_size(
    prompt: &str,
    ctx_size_arg: Option<&str>,
    model_max_context: Option<u64>,
) -> Result<ContextResolution> {
    // If user explicitly provided context size, use resolve_context_size
    if ctx_size_arg.is_some() || model_max_context.is_some() {
        return resolve_context_size(ctx_size_arg.map(String::from), model_max_context);
    }

    // Estimate token count (chars / 4 is a standard approximation)
    let estimated_prompt_tokens = (prompt.len() / 4) as u64;
    // Add buffer for the response (2048 tokens for generation)
    let total_needed = estimated_prompt_tokens + 2048;

    // Use estimated or 4096 minimum
    let context_size = total_needed.max(4096);

    resolve_context_size(Some(context_size.to_string()), model_max_context)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_prompt_with_placeholder() {
        let question = "Summarize this: {}";
        let input = "Hello world";
        let prompt = build_prompt(question, Some(input)).unwrap();
        assert_eq!(prompt, "Summarize this: Hello world");
    }

    #[test]
    fn test_build_prompt_without_placeholder() {
        let question = "What does this mean?";
        let input = "Hello world";
        let prompt = build_prompt(question, Some(input)).unwrap();
        assert!(prompt.contains("Context:"));
        assert!(prompt.contains("Hello world"));
        assert!(prompt.contains("What does this mean?"));
    }

    #[test]
    fn test_build_prompt_no_input() {
        let question = "What is Rust?";
        let prompt = build_prompt(question, None).unwrap();
        assert_eq!(prompt, "What is Rust?");
    }

    #[test]
    fn test_build_prompt_empty_input() {
        let question = "What is Rust?";
        let prompt = build_prompt(question, Some("")).unwrap();
        assert_eq!(prompt, "What is Rust?");
    }

    #[test]
    fn test_calculate_context_size_explicit() {
        let result = calculate_context_size("test", Some("8192"), Some(16384)).unwrap();
        assert_eq!(result.value, Some(8192));
    }

    #[test]
    fn test_calculate_context_size_auto() {
        // When model_max_context is provided but no ctx_size_arg, it early-returns
        // to resolve_context_size which returns None
        let result = calculate_context_size("test", None, Some(16384)).unwrap();
        assert_eq!(result.value, None);
    }

    #[test]
    fn test_calculate_context_size_large_prompt() {
        // When BOTH are None, it does the estimation
        let large_prompt = "x".repeat(10000);
        let result = calculate_context_size(&large_prompt, None, None).unwrap();
        // Should use estimated size (2500 + 2048 = 4548, rounded up to meet minimum)
        assert!(result.value.unwrap() >= 4548);
    }

    #[test]
    fn test_calculate_context_size_no_model_max() {
        // When no model max is known and no explicit ctx, should estimate
        let result = calculate_context_size("test prompt", None, None).unwrap();
        // Minimum is 4096
        assert_eq!(result.value, Some(4096));
    }

    #[test]
    fn test_calculate_context_size_explicit_max_keyword() {
        // User can specify "max" to use model's maximum
        let result = calculate_context_size("test", Some("max"), Some(32768)).unwrap();
        assert_eq!(result.value, Some(32768));
    }

    #[test]
    fn test_build_prompt_multiple_placeholders() {
        // .replace() replaces ALL occurrences
        let question = "Compare {} with {}";
        let input = "Hello";
        let prompt = build_prompt(question, Some(input)).unwrap();
        assert_eq!(prompt, "Compare Hello with Hello");
    }

    #[test]
    fn test_build_prompt_context_format() {
        // Verify exact format when no placeholder
        let question = "What is this?";
        let input = "Some code here";
        let prompt = build_prompt(question, Some(input)).unwrap();
        assert_eq!(prompt, "Context:\nSome code here\n\nQuestion: What is this?\n\nAnswer:");
    }

    #[test]
    fn test_build_prompt_whitespace_input() {
        // Whitespace-only input is NOT treated as empty (is_empty() returns false)
        // It goes through the context path with trimmed content
        let question = "What is Rust?";
        let prompt = build_prompt(question, Some("   \n\t  ")).unwrap();
        // After trimming the whitespace in the format, we get empty context
        assert_eq!(prompt, "Context:\n\n\nQuestion: What is Rust?\n\nAnswer:");
    }
}
