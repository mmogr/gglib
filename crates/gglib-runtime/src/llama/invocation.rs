//! Shared llama command invocation builder.
//!
//! This module provides a DRY abstraction for building llama-cli and llama-server
//! command invocations, eliminating duplication between chat, serve, and other commands.

use super::args::{ContextResolution, ContextResolutionSource};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Builder for constructing llama-cli or llama-server commands.
///
/// This builder handles common flags (`-m`, `-c`, `--mlock`) while allowing
/// callers to customize with command-specific flags.
///
/// # Example
///
/// ```rust,ignore
/// let cmd = LlamaCommandBuilder::new(llama_path, model_path)
///     .context_resolution(ctx_resolution)
///     .mlock(true)
///     .arg("--port", Some("8080"))
///     .build();
/// ```
pub struct LlamaCommandBuilder {
    binary_path: PathBuf,
    model_path: PathBuf,
    context_resolution: Option<ContextResolution>,
    mlock: bool,
    additional_args: Vec<(String, Option<String>)>,
}

impl LlamaCommandBuilder {
    /// Create a new builder with the required binary and model paths.
    pub fn new(binary_path: impl Into<PathBuf>, model_path: impl Into<PathBuf>) -> Self {
        Self {
            binary_path: binary_path.into(),
            model_path: model_path.into(),
            context_resolution: None,
            mlock: false,
            additional_args: Vec::new(),
        }
    }

    /// Set the context size resolution result.
    pub fn context_resolution(mut self, resolution: ContextResolution) -> Self {
        self.context_resolution = Some(resolution);
        self
    }

    /// Enable or disable memory lock.
    pub fn mlock(mut self, enabled: bool) -> Self {
        self.mlock = enabled;
        self
    }

    /// Add an additional flag with an optional value.
    ///
    /// # Arguments
    /// * `key` - The flag name (e.g., "--port", "--host")
    /// * `value` - Optional value for the flag
    pub fn arg(mut self, key: impl Into<String>, value: Option<impl Into<String>>) -> Self {
        self.additional_args
            .push((key.into(), value.map(|v| v.into())));
        self
    }

    /// Add an additional flag with a required value.
    pub fn arg_with_value(self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.arg(key, Some(value))
    }

    /// Add a boolean flag (flag with no value).
    pub fn flag(self, key: impl Into<String>) -> Self {
        self.arg(key, None::<String>)
    }

    /// Build the final Command ready for execution.
    ///
    /// The command is constructed with:
    /// 1. Model path (`-m`)
    /// 2. Context size (`-c`) if resolved
    /// 3. Memory lock (`--mlock`) if enabled
    /// 4. Additional flags in order they were added
    pub fn build(self) -> Command {
        let mut cmd = Command::new(&self.binary_path);

        // Model path (always required)
        cmd.arg("-m").arg(&self.model_path);

        // Context size (if resolved)
        if let Some(ContextResolution {
            value: Some(size), ..
        }) = self.context_resolution
        {
            cmd.arg("-c").arg(size.to_string());
        }

        // Memory lock
        if self.mlock {
            cmd.arg("--mlock");
        }

        // Additional flags
        for (key, value) in self.additional_args {
            cmd.arg(key);
            if let Some(val) = value {
                cmd.arg(val);
            }
        }

        cmd
    }

    /// Get the context resolution source for logging purposes.
    pub fn get_context_source(&self) -> Option<ContextResolutionSource> {
        self.context_resolution.map(|r| r.source)
    }

    /// Get the resolved context value for logging purposes.
    pub fn get_context_value(&self) -> Option<u32> {
        self.context_resolution.and_then(|r| r.value)
    }
}

/// Print standardized context size information to stdout.
///
/// This provides consistent UX across commands when displaying context information.
pub fn log_context_info(resolution: &ContextResolution) {
    match resolution.source {
        ContextResolutionSource::ExplicitFlag => {
            if let Some(size) = resolution.value {
                println!("Using context size: {}", size);
            }
        }
        ContextResolutionSource::ModelMetadata => {
            if let Some(size) = resolution.value {
                println!("Using maximum context size from model: {}", size);
            }
        }
        ContextResolutionSource::MaxRequestedMissing => {
            println!("Warning: 'max' specified but no context length found in model metadata");
        }
        ContextResolutionSource::NotSpecified => {}
    }
}

/// Print standardized model information to stdout.
///
/// # Arguments
/// * `model_name` - Display name of the model
/// * `model_path` - Path to the model file
/// * `binary_name` - Name of the binary being started (e.g., "llama-server", "llama-cli")
pub fn log_model_info(model_name: &str, model_path: &Path, binary_name: &str) {
    println!("Starting {} with model: {}", binary_name, model_name);
    println!("Model path: {}", model_path.display());
}

/// Print standardized mlock information to stdout.
pub fn log_mlock_info(enabled: bool) {
    if enabled {
        println!("Enabling memory lock");
    }
}

/// Print the command being executed to stdout.
///
/// Formats the command args in a readable way for the user.
pub fn log_command_execution(cmd: &Command) {
    let args: Vec<_> = cmd.get_args().map(|arg| arg.to_string_lossy()).collect();

    if args.is_empty() {
        println!("Executing: {}", cmd.get_program().to_string_lossy());
    } else {
        println!(
            "Executing: {} {}",
            cmd.get_program().to_string_lossy(),
            args.join(" ")
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builder_constructs_basic_command() {
        let cmd = LlamaCommandBuilder::new("/usr/bin/llama-cli", "/models/test.gguf").build();

        let args: Vec<_> = cmd.get_args().map(|a| a.to_string_lossy()).collect();
        assert!(args.contains(&"-m".into()));
        assert!(args.contains(&"/models/test.gguf".into()));
    }

    #[test]
    fn builder_adds_context_size() {
        let resolution = ContextResolution {
            value: Some(4096),
            source: ContextResolutionSource::ExplicitFlag,
        };

        let cmd = LlamaCommandBuilder::new("/usr/bin/llama-cli", "/models/test.gguf")
            .context_resolution(resolution)
            .build();

        let args: Vec<_> = cmd.get_args().map(|a| a.to_string_lossy()).collect();
        assert!(args.contains(&"-c".into()));
        assert!(args.contains(&"4096".into()));
    }

    #[test]
    fn builder_adds_mlock() {
        let cmd = LlamaCommandBuilder::new("/usr/bin/llama-cli", "/models/test.gguf")
            .mlock(true)
            .build();

        let args: Vec<_> = cmd.get_args().map(|a| a.to_string_lossy()).collect();
        assert!(args.contains(&"--mlock".into()));
    }

    #[test]
    fn builder_adds_custom_flags() {
        let cmd = LlamaCommandBuilder::new("/usr/bin/llama-server", "/models/test.gguf")
            .arg("--port", Some("8080"))
            .arg("--host", Some("127.0.0.1"))
            .flag("--api")
            .build();

        let args: Vec<_> = cmd.get_args().map(|a| a.to_string_lossy()).collect();
        assert!(args.contains(&"--port".into()));
        assert!(args.contains(&"8080".into()));
        assert!(args.contains(&"--host".into()));
        assert!(args.contains(&"127.0.0.1".into()));
        assert!(args.contains(&"--api".into()));
    }

    #[test]
    fn builder_preserves_flag_order() {
        let cmd = LlamaCommandBuilder::new("/usr/bin/llama-cli", "/models/test.gguf")
            .arg("--first", Some("1"))
            .arg("--second", Some("2"))
            .arg("--third", Some("3"))
            .build();

        let args: Vec<_> = cmd.get_args().map(|a| a.to_string_lossy()).collect();

        // Find positions (after -m and model path)
        let first_pos = args.iter().position(|a| a == "--first");
        let second_pos = args.iter().position(|a| a == "--second");
        let third_pos = args.iter().position(|a| a == "--third");

        assert!(first_pos < second_pos);
        assert!(second_pos < third_pos);
    }

    #[test]
    fn builder_combines_all_features() {
        let resolution = ContextResolution {
            value: Some(8192),
            source: ContextResolutionSource::ModelMetadata,
        };

        let cmd = LlamaCommandBuilder::new("/usr/bin/llama-server", "/models/test.gguf")
            .context_resolution(resolution)
            .mlock(true)
            .arg("--port", Some("8080"))
            .flag("--api")
            .build();

        let args: Vec<_> = cmd.get_args().map(|a| a.to_string_lossy()).collect();

        // Verify all components present
        assert!(args.contains(&"-m".into()));
        assert!(args.contains(&"/models/test.gguf".into()));
        assert!(args.contains(&"-c".into()));
        assert!(args.contains(&"8192".into()));
        assert!(args.contains(&"--mlock".into()));
        assert!(args.contains(&"--port".into()));
        assert!(args.contains(&"8080".into()));
        assert!(args.contains(&"--api".into()));
    }

    #[test]
    fn builder_getters_work() {
        let resolution = ContextResolution {
            value: Some(4096),
            source: ContextResolutionSource::ExplicitFlag,
        };

        let builder = LlamaCommandBuilder::new("/usr/bin/llama-cli", "/models/test.gguf")
            .context_resolution(resolution);

        assert_eq!(builder.get_context_value(), Some(4096));
        assert_eq!(
            builder.get_context_source(),
            Some(ContextResolutionSource::ExplicitFlag)
        );
    }
}
