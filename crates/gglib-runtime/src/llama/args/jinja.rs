//! Jinja template flag resolution for llama.cpp launches.

/// Indicates how the Jinja flag was resolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JinjaResolutionSource {
    /// User explicitly forced Jinja on via CLI/UI flag.
    ExplicitTrue,
    /// User explicitly disabled Jinja even if tags would auto-enable it.
    ExplicitFalse,
    /// Auto-enabled because the model has the "agent" tag.
    AgentTag,
    /// Not enabled (default).
    Default,
}

/// Result of resolving whether to enable Jinja templates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JinjaResolution {
    /// Whether the `--jinja` flag should be forwarded to llama.cpp.
    pub enabled: bool,
    /// Source of the decision, used for UX/logging.
    pub source: JinjaResolutionSource,
}

/// Determine whether to enable Jinja templates for llama-server launches.
pub fn resolve_jinja_flag(explicit: Option<bool>, tags: &[String]) -> JinjaResolution {
    match explicit {
        Some(true) => JinjaResolution {
            enabled: true,
            source: JinjaResolutionSource::ExplicitTrue,
        },
        Some(false) => JinjaResolution {
            enabled: false,
            source: JinjaResolutionSource::ExplicitFalse,
        },
        None => {
            if tags.iter().any(|tag| tag.eq_ignore_ascii_case("agent")) {
                JinjaResolution {
                    enabled: true,
                    source: JinjaResolutionSource::AgentTag,
                }
            } else {
                JinjaResolution {
                    enabled: false,
                    source: JinjaResolutionSource::Default,
                }
            }
        }
    }
}
