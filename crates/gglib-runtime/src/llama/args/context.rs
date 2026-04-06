//! Context size resolution for llama.cpp launches.
//!
//! Provides a single [`resolve_context_size`] function that every surface
//! (CLI serve / chat / question, GUI backend) must call to determine the
//! context window forwarded to llama-server.
//!
//! ## Precedence (highest → lowest)
//!
//! 1. **Explicit flag / request field** — user passed `-c 8192` or `--ctx-size max`.
//! 2. **Global settings default** — `Settings.default_context_size` from the database.
//! 3. **Llama-server built-in default** — no value forwarded; server decides.

use anyhow::{Result, anyhow};

/// Indicates how a context size value was resolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextResolutionSource {
    /// User passed an explicit numeric flag.
    ExplicitFlag,
    /// User asked for `max` and we used the model metadata.
    ModelMetadata,
    /// Value came from the persisted global settings default.
    SettingsDefault,
    /// The flag was omitted and no settings default exists.
    NotSpecified,
    /// User asked for `max` but the metadata did not contain a value.
    MaxRequestedMissing,
}

/// Inputs needed to resolve a context size.
///
/// Bundled into a struct to keep the [`resolve_context_size`] signature
/// clean as the number of inputs grows.
#[derive(Debug, Clone, Default)]
pub struct ContextInput {
    /// Raw CLI flag value (numeric string or `"max"`).
    pub flag: Option<String>,
    /// Context length from model GGUF metadata (used when flag is `"max"`).
    pub model_context_length: Option<u64>,
    /// Persisted global default from `Settings.default_context_size`.
    pub settings_default: Option<u64>,
}

/// Result of resolving a context size flag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContextResolution {
    /// The numeric value to forward to llama.cpp (if any).
    pub value: Option<u32>,
    /// Indicates where the value came from for logging UX.
    pub source: ContextResolutionSource,
}

/// Resolve the effective context size from all available sources.
///
/// See the [module-level docs](self) for the full precedence chain.
pub fn resolve_context_size(input: ContextInput) -> Result<ContextResolution> {
    match input.flag {
        Some(raw) => resolve_explicit_flag(&raw, input.model_context_length),
        None => resolve_from_settings(input.settings_default),
    }
}

/// Handle an explicit `--ctx-size` value (numeric or `"max"`).
fn resolve_explicit_flag(
    raw: &str,
    model_context_length: Option<u64>,
) -> Result<ContextResolution> {
    let value = raw.trim();
    if value.eq_ignore_ascii_case("max") {
        return resolve_max(model_context_length);
    }

    let ctx_value: u32 = value.parse().map_err(|_| {
        anyhow!(
            "Invalid context size '{}'. Use a positive number or 'max'",
            value
        )
    })?;

    Ok(ContextResolution {
        value: Some(ctx_value),
        source: ContextResolutionSource::ExplicitFlag,
    })
}

/// Resolve `--ctx-size max` against model GGUF metadata.
fn resolve_max(model_context_length: Option<u64>) -> Result<ContextResolution> {
    if let Some(model_ctx) = model_context_length {
        let ctx_u32 = u32::try_from(model_ctx).map_err(|_| {
            anyhow!(
                "Model context length {} exceeds supported range for llama.cpp",
                model_ctx
            )
        })?;
        Ok(ContextResolution {
            value: Some(ctx_u32),
            source: ContextResolutionSource::ModelMetadata,
        })
    } else {
        Ok(ContextResolution {
            value: None,
            source: ContextResolutionSource::MaxRequestedMissing,
        })
    }
}

/// Fall back to the persisted settings default when no flag was given.
fn resolve_from_settings(settings_default: Option<u64>) -> Result<ContextResolution> {
    if let Some(default) = settings_default {
        let ctx_u32 = u32::try_from(default).map_err(|_| {
            anyhow!(
                "Settings default context size {} exceeds supported range for llama.cpp",
                default
            )
        })?;
        Ok(ContextResolution {
            value: Some(ctx_u32),
            source: ContextResolutionSource::SettingsDefault,
        })
    } else {
        Ok(ContextResolution {
            value: None,
            source: ContextResolutionSource::NotSpecified,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_numeric_wins_over_settings() {
        let res = resolve_context_size(ContextInput {
            flag: Some("8192".into()),
            settings_default: Some(4096),
            ..Default::default()
        })
        .unwrap();
        assert_eq!(res.value, Some(8192));
        assert_eq!(res.source, ContextResolutionSource::ExplicitFlag);
    }

    #[test]
    fn max_uses_model_metadata() {
        let res = resolve_context_size(ContextInput {
            flag: Some("max".into()),
            model_context_length: Some(131072),
            settings_default: Some(4096),
            ..Default::default()
        })
        .unwrap();
        assert_eq!(res.value, Some(131072));
        assert_eq!(res.source, ContextResolutionSource::ModelMetadata);
    }

    #[test]
    fn no_flag_falls_back_to_settings() {
        let res = resolve_context_size(ContextInput {
            flag: None,
            settings_default: Some(16384),
            ..Default::default()
        })
        .unwrap();
        assert_eq!(res.value, Some(16384));
        assert_eq!(res.source, ContextResolutionSource::SettingsDefault);
    }

    #[test]
    fn no_flag_no_settings_returns_not_specified() {
        let res = resolve_context_size(ContextInput::default()).unwrap();
        assert_eq!(res.value, None);
        assert_eq!(res.source, ContextResolutionSource::NotSpecified);
    }

    #[test]
    fn max_without_metadata_returns_missing() {
        let res = resolve_context_size(ContextInput {
            flag: Some("max".into()),
            model_context_length: None,
            ..Default::default()
        })
        .unwrap();
        assert_eq!(res.value, None);
        assert_eq!(res.source, ContextResolutionSource::MaxRequestedMissing);
    }

    #[test]
    fn invalid_flag_returns_error() {
        let res = resolve_context_size(ContextInput {
            flag: Some("banana".into()),
            ..Default::default()
        });
        assert!(res.is_err());
    }
}
