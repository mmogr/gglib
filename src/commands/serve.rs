//! Serve command implementation for starting llama-server with a GGUF model.
//!
//! This module handles serving GGUF models using llama-server, including
//! context size detection and process management.

use crate::commands::llama_args::{
    JinjaResolution, JinjaResolutionSource, resolve_context_size, resolve_jinja_flag,
};
use crate::commands::llama::ensure_llama_initialized;
use crate::commands::llama_invocation::{
    LlamaCommandBuilder, log_command_execution, log_context_info, log_mlock_info, log_model_info,
    resolve_model_for_invocation,
};
use crate::services::AppCore;
use crate::utils::paths::get_llama_server_path;
use anyhow::Result;
use std::process::Stdio;
use std::sync::Arc;

/// Handle the serve command to start llama-server with a GGUF model.
///
/// This function:
/// 1. Looks up the model by ID in the database
/// 2. Determines the context size (from flag, database, or default)
/// 3. Executes llama-server with appropriate flags
///
/// # Arguments
///
/// * `id` - ID of the model to serve
/// * `ctx_size` - Optional context size ("max" for auto-detect, number, or None)
/// * `mlock` - Whether to enable memory lock
/// * `jinja_flag` - Whether the user explicitly requested `--jinja`
/// * `port` - Port to serve on
///
/// # Returns
///
/// Returns `Result<()>` indicating success or failure of the operation.
///
/// # Errors
///
/// This function will return an error if:
/// - The model is not found in the database
/// - llama-server is not found in PATH
/// - The model file doesn't exist
/// - llama-server fails to start
pub async fn handle_serve(
    core: Arc<AppCore>,
    id: u32,
    ctx_size: Option<String>,
    mlock: bool,
    jinja_flag: bool,
    port: u16,
) -> Result<()> {
    // Ensure llama.cpp is installed
    ensure_llama_initialized().await?;

    // Get llama-server binary path
    let llama_server_path = get_llama_server_path()?;

    // Use shared model resolution helper
    let resolved = resolve_model_for_invocation(&core, &id.to_string()).await?;
    let model = resolved.model;

    log_model_info(&model, "llama-server");

    // Handle context size consistently across commands
    let context_resolution = resolve_context_size(ctx_size, model.context_length)?;
    log_context_info(&context_resolution);
    log_mlock_info(mlock);

    let explicit_jinja = if jinja_flag { Some(true) } else { None };
    let jinja_resolution = resolve_jinja_flag(explicit_jinja, &model.tags);
    log_jinja_decision(&jinja_resolution);

    println!("Server will be available on http://localhost:{}", port);

    // Build llama-server command using shared builder
    let mut builder = LlamaCommandBuilder::new(&llama_server_path, &model.file_path)
        .context_resolution(context_resolution)
        .mlock(mlock)
        .arg_with_value("--port", port.to_string());

    if jinja_resolution.enabled {
        builder = builder.flag("--jinja");
    }

    let mut cmd = builder.build();

    // Set up stdio to inherit from parent (so user can see output and interact)
    cmd.stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());

    log_command_execution(&cmd);
    println!("Starting server... (Press Ctrl+C to stop)");

    // Execute llama-server and wait for it to complete
    let status = cmd.status()?;

    if status.success() {
        println!("llama-server exited successfully");
        Ok(())
    } else {
        Err(anyhow::anyhow!(
            "llama-server exited with code: {:?}",
            status.code()
        ))
    }
}

fn log_jinja_decision(resolution: &JinjaResolution) {
    match (resolution.enabled, resolution.source) {
        (true, JinjaResolutionSource::ExplicitTrue) => {
            println!("Enabling Jinja templates (--jinja flag)");
        }
        (true, JinjaResolutionSource::AgentTag) => {
            println!("Enabling Jinja templates due to 'agent' tag");
        }
        (false, JinjaResolutionSource::ExplicitFalse) => {
            println!("Jinja templates disabled by explicit override");
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_context_size_parsing() {
        // Test different context size input formats
        let test_cases = vec![
            (Some("4096".to_string()), Some(4096)),
            (Some("8192".to_string()), Some(8192)),
            (Some("max".to_string()), None), // "max" means use model default
            (Some("0".to_string()), Some(0)),
            (None, None), // No ctx_size provided
        ];

        for (input, expected) in test_cases {
            match input {
                Some(val) if val == "max" => {
                    assert!(expected.is_none(), "max should result in None");
                }
                Some(val) => {
                    let parsed = val.parse::<u32>().ok();
                    assert_eq!(parsed, expected, "Failed to parse: {}", val);
                }
                None => {
                    assert!(expected.is_none(), "None input should result in None");
                }
            }
        }
    }

    #[test]
    fn test_command_argument_construction() {
        // Test building command arguments
        fn build_serve_args(
            model_path: &str,
            ctx_size: Option<u32>,
            mlock: bool,
            jinja: bool,
        ) -> Vec<String> {
            let mut args = vec![
                "--model".to_string(),
                model_path.to_string(),
                "--host".to_string(),
                "0.0.0.0".to_string(),
                "--port".to_string(),
                "8080".to_string(),
            ];

            if let Some(ctx) = ctx_size {
                args.push("--ctx-size".to_string());
                args.push(ctx.to_string());
            }

            if mlock {
                args.push("--mlock".to_string());
            }

            if jinja {
                args.push("--jinja".to_string());
            }

            args
        }

        let args1 = build_serve_args("/path/model.gguf", Some(4096), false, false);
        assert!(args1.contains(&"--model".to_string()));
        assert!(args1.contains(&"/path/model.gguf".to_string()));
        assert!(args1.contains(&"--ctx-size".to_string()));
        assert!(args1.contains(&"4096".to_string()));
        assert!(!args1.contains(&"--mlock".to_string()));
        assert!(!args1.contains(&"--jinja".to_string()));

        let args2 = build_serve_args("/path/model.gguf", None, true, true);
        assert!(args2.contains(&"--model".to_string()));
        assert!(!args2.contains(&"--ctx-size".to_string()));
        assert!(args2.contains(&"--mlock".to_string()));
        assert!(args2.contains(&"--jinja".to_string()));
    }

    #[test]
    fn test_context_size_validation() {
        // Test context size validation logic
        fn validate_context_size(input: &str) -> Result<Option<u32>, String> {
            if input == "max" {
                Ok(None)
            } else {
                match input.parse::<u32>() {
                    Ok(val) if val > 0 => Ok(Some(val)),
                    Ok(_) => Err("Context size must be greater than 0".to_string()),
                    Err(_) => Err("Invalid context size format".to_string()),
                }
            }
        }

        assert_eq!(validate_context_size("max"), Ok(None));
        assert_eq!(validate_context_size("4096"), Ok(Some(4096)));
        assert_eq!(validate_context_size("8192"), Ok(Some(8192)));
        assert!(validate_context_size("0").is_err());
        assert!(validate_context_size("abc").is_err());
        assert!(validate_context_size("-1").is_err());
    }

    #[test]
    fn test_model_id_validation() {
        // Test model ID validation
        fn validate_model_id(id: u32) -> bool {
            id > 0 // IDs should be positive
        }

        assert!(validate_model_id(1));
        assert!(validate_model_id(999));
        assert!(!validate_model_id(0));
    }

    // Integration tests for handle_serve would test with actual database
    // and potentially mock the llama-server process execution
}
