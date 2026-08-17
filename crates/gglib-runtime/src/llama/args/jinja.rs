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
//! This used to be one `enabled: bool`, where `false` meant "emit nothing".
//! That silently discarded the first case: an explicit jinja-off produced a
//! server running with jinja, and nothing said so. [`JinjaMode`] names the
//! three answers so the emitter cannot conflate them again.
//!
//! Note the consequence for the second case: an untagged model with no
//! override gets jinja from upstream. gglib does not take that away — doing so
//! would newly break tool-call templating and template kwargs for every
//! non-agent model.

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
/// See the module docs for why the three outcomes are not two.
#[must_use]
pub fn resolve_jinja_flag(explicit: Option<bool>, tags: &[String]) -> JinjaResolution {
    match explicit {
        Some(true) => JinjaResolution {
            mode: JinjaMode::On,
            source: JinjaResolutionSource::ExplicitTrue,
        },
        Some(false) => JinjaResolution {
            mode: JinjaMode::Off,
            source: JinjaResolutionSource::ExplicitFalse,
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

    /// The bug this module was rewritten for: an explicit false used to
    /// resolve to `enabled: false`, which emitted nothing, which left the
    /// server running with upstream's jinja-on default.
    #[test]
    fn an_explicit_false_resolves_to_off_not_to_silence() {
        let r = resolve_jinja_flag(Some(false), &tags(&["agent"]));
        assert_eq!(r.mode, JinjaMode::Off);
        assert_eq!(r.source, JinjaResolutionSource::ExplicitFalse);
    }

    #[test]
    fn an_explicit_true_resolves_to_on() {
        let r = resolve_jinja_flag(Some(true), &[]);
        assert_eq!(r.mode, JinjaMode::On);
        assert_eq!(r.source, JinjaResolutionSource::ExplicitTrue);
    }

    #[test]
    fn the_agent_tag_resolves_to_on() {
        let r = resolve_jinja_flag(None, &tags(&["reasoning", "agent"]));
        assert_eq!(r.mode, JinjaMode::On);
        assert_eq!(r.source, JinjaResolutionSource::AgentTag);
    }

    /// Tags reach the catalog from GGUF detection, `HuggingFace` metadata and
    /// hand edits, and only the first is guaranteed lowercase. Preserved from
    /// the hand-rolled `eq_ignore_ascii_case` this now delegates to
    /// [`capability_tags::has`].
    #[test]
    fn the_agent_tag_match_is_case_insensitive() {
        assert_eq!(
            resolve_jinja_flag(None, &tags(&["Agent"])).mode,
            JinjaMode::On
        );
    }

    /// Untagged and un-overridden is a *deferral*, not an off. Turning jinja
    /// off here would take tool-call templating and template kwargs away from
    /// every non-agent model — a behaviour change this resolver deliberately
    /// does not make.
    #[test]
    fn an_untagged_model_defers_rather_than_disabling() {
        let r = resolve_jinja_flag(None, &tags(&["reasoning", "code"]));
        assert_eq!(r.mode, JinjaMode::Defer);
        assert_eq!(r.source, JinjaResolutionSource::Default);
    }

    /// An explicit false beats the tag that would otherwise auto-enable —
    /// the whole point of an override.
    #[test]
    fn an_explicit_false_overrides_the_agent_tag() {
        assert_eq!(
            resolve_jinja_flag(Some(false), &tags(&["agent", "mtp"])).mode,
            JinjaMode::Off
        );
    }
}
