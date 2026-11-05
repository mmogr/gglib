//! Jinja template flag resolution for llama.cpp launches.
//!
//! # Why the result is not a bool
//!
//! llama-server's own default is jinja **on** — `use_jinja` initialises to
//! `true` in `common/common.h:621`, and `common/arg.cpp:1394-1399` flips it
//! false only for the completion and mtmd examples, never for the server. So
//! there are two distinct ways for this resolver to answer "not on", and they
//! call for opposite launches:
//!
//! - the user explicitly turned jinja off, which needs `--no-jinja` to be
//!   emitted or the server runs with jinja regardless;
//! - gglib has no opinion — no `agent` tag, no override — which needs *no*
//!   flag, leaving upstream's default in place.
//!
//! A bool whose `false` meant "emit nothing" would discard the first case
//! silently: an explicit jinja-off would produce a server running with jinja.
//! [`JinjaMode`] names the three answers so the emitter cannot conflate them.
//!
//! Note the consequence for the second case: an untagged model with no
//! override gets jinja from upstream. gglib does not take that away — doing so
//! would break tool-call templating and template kwargs for every non-agent
//! model.

use gglib_core::domain::capability_tags;
use gglib_core::ports::JinjaMode;

/// Indicates how the Jinja flag was resolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JinjaResolutionSource {
    /// User explicitly forced Jinja on via CLI/UI flag.
    ExplicitTrue,
    /// User explicitly disabled Jinja even if tags would auto-enable it.
    ///
    /// The only source that yields [`JinjaMode::Off`], and therefore the only
    /// one that puts `--no-jinja` on the command line.
    ExplicitFalse,
    /// The model's own default (`ServerConfig::jinja`), set on the model
    /// with `gglib model update --jinja` or in the inspector. Either
    /// direction: it is the one rung besides an explicit choice that can
    /// put `--no-jinja` on the command line.
    ModelDefault,
    /// Auto-enabled because the model has the
    /// [`AGENT`](capability_tags::AGENT) tag.
    AgentTag,
    /// No tag and no override, so gglib takes no position.
    ///
    /// Not the same as "off": this yields [`JinjaMode::Defer`], gglib emits no
    /// flag, and llama-server's own default (jinja on) applies.
    Default,
}

/// Result of resolving what a launch says about Jinja templates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JinjaResolution {
    /// Which jinja flag, if any, should be forwarded to llama.cpp.
    pub mode: JinjaMode,
    /// Source of the decision, used for UX/logging.
    pub source: JinjaResolutionSource,
}

/// Determine what a llama-server launch says about Jinja templates.
///
/// The launch's own choice first, then the model's own default, then the
/// tags. See the module docs for why the three outcomes are not two.
#[must_use]
pub fn resolve_jinja_flag(
    explicit: Option<bool>,
    model_default: Option<bool>,
    tags: &[String],
) -> JinjaResolution {
    match explicit {
        Some(true) => JinjaResolution {
            mode: JinjaMode::On,
            source: JinjaResolutionSource::ExplicitTrue,
        },
        Some(false) => JinjaResolution {
            mode: JinjaMode::Off,
            source: JinjaResolutionSource::ExplicitFalse,
        },
        None if model_default.is_some() => JinjaResolution {
            mode: if model_default == Some(true) {
                JinjaMode::On
            } else {
                JinjaMode::Off
            },
            source: JinjaResolutionSource::ModelDefault,
        },
        None => {
            if capability_tags::has(tags, capability_tags::AGENT) {
                JinjaResolution {
                    mode: JinjaMode::On,
                    source: JinjaResolutionSource::AgentTag,
                }
            } else {
                JinjaResolution {
                    mode: JinjaMode::Defer,
                    source: JinjaResolutionSource::Default,
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tags(names: &[&str]) -> Vec<String> {
        names.iter().map(|s| (*s).to_string()).collect()
    }

    /// An explicit false resolves to `Off`, which emits `--no-jinja`, and not
    /// to silence, which would leave the server running with upstream's
    /// jinja-on default.
    #[test]
    fn an_explicit_false_resolves_to_off_not_to_silence() {
        let r = resolve_jinja_flag(Some(false), None, &tags(&["agent"]));
        assert_eq!(r.mode, JinjaMode::Off);
        assert_eq!(r.source, JinjaResolutionSource::ExplicitFalse);
    }

    #[test]
    fn an_explicit_true_resolves_to_on() {
        let r = resolve_jinja_flag(Some(true), None, &[]);
        assert_eq!(r.mode, JinjaMode::On);
        assert_eq!(r.source, JinjaResolutionSource::ExplicitTrue);
    }

    #[test]
    fn the_agent_tag_resolves_to_on() {
        let r = resolve_jinja_flag(None, None, &tags(&["reasoning", "agent"]));
        assert_eq!(r.mode, JinjaMode::On);
        assert_eq!(r.source, JinjaResolutionSource::AgentTag);
    }

    /// Tags reach the catalog from GGUF detection, `HuggingFace` metadata and
    /// hand edits, and only the first is guaranteed lowercase, so the match
    /// goes through [`capability_tags::has`], which ignores ASCII case.
    #[test]
    fn the_agent_tag_match_is_case_insensitive() {
        assert_eq!(
            resolve_jinja_flag(None, None, &tags(&["Agent"])).mode,
            JinjaMode::On
        );
    }

    /// Untagged and un-overridden is a *deferral*, not an off. Turning jinja
    /// off here would take tool-call templating and template kwargs away from
    /// every non-agent model — a behaviour change this resolver deliberately
    /// does not make.
    #[test]
    fn an_untagged_model_defers_rather_than_disabling() {
        let r = resolve_jinja_flag(None, None, &tags(&["reasoning", "code"]));
        assert_eq!(r.mode, JinjaMode::Defer);
        assert_eq!(r.source, JinjaResolutionSource::Default);
    }

    /// An explicit false beats the tag that would otherwise auto-enable —
    /// the whole point of an override.
    #[test]
    fn an_explicit_false_overrides_the_agent_tag() {
        assert_eq!(
            resolve_jinja_flag(Some(false), None, &tags(&["agent", "mtp"])).mode,
            JinjaMode::Off
        );
    }

    /// The model's own default sits between a launch's choice and the tags:
    /// it turns an agent-tagged model's templates off, and a launch's
    /// choice turns them back on.
    #[test]
    fn the_models_own_default_outranks_the_tags_and_yields_to_the_launch() {
        let off = resolve_jinja_flag(None, Some(false), &tags(&["agent"]));
        assert_eq!(off.mode, JinjaMode::Off);
        assert_eq!(off.source, JinjaResolutionSource::ModelDefault);

        let on = resolve_jinja_flag(None, Some(true), &[]);
        assert_eq!(on.mode, JinjaMode::On);
        assert_eq!(on.source, JinjaResolutionSource::ModelDefault);

        let launch = resolve_jinja_flag(Some(true), Some(false), &[]);
        assert_eq!(launch.mode, JinjaMode::On);
        assert_eq!(launch.source, JinjaResolutionSource::ExplicitTrue);
    }
}
