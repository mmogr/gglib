//! Context size resolution for llama.cpp launches.

use anyhow::{Result, anyhow};

/// Indicates how a context size value was resolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextResolutionSource {
    /// User passed an explicit numeric flag.
    ExplicitFlag,
    /// User asked for `max` and we used the model metadata.
    ModelMetadata,
    /// The flag was omitted entirely.
    NotSpecified,
    /// User asked for `max` but the metadata did not contain a value.
    MaxRequestedMissing,
}

/// Result of resolving a context size flag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContextResolution {
    /// The numeric value to forward to llama.cpp (if any).
    pub value: Option<u32>,
    /// Indicates where the value came from for logging UX.
    pub source: ContextResolutionSource,
}

/// Normalize a context-size input ("max" or numeric string).
pub fn resolve_context_size(
    ctx_flag: Option<String>,
    model_context_length: Option<u64>,
) -> Result<ContextResolution> {
    match ctx_flag {
        Some(raw) => {
            let value = raw.trim();
            if value.eq_ignore_ascii_case("max") {
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
            } else {
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
        }
        None => Ok(ContextResolution {
            value: None,
            source: ContextResolutionSource::NotSpecified,
        }),
    }
}
