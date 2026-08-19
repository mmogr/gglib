//! llama-server's per-template capability self-report, and the tri-state
//! gglib holds it in.
//!
//! [ADR 0007]'s observation: the pinned build computes, per loaded chat
//! template, a `chat_template_caps` structure (`jinja::caps`,
//! `common/jinja/caps.h`) by **executing the template with instrumented
//! variable access**, and publishes the result unconditionally on
//! `GET /props`. gglib reads that self-report rather than building a
//! detector of its own — a gglib reimplementation could only ever *disagree*
//! with the renderer it is trying to predict, and every disagreement would be
//! a bug on gglib's side by construction.
//!
//! # A report, not a conservative baseline
//!
//! Five of the nine bools default `true` upstream (`caps.h:11-14,23`), so an
//! absent field must never be read as `false` — which is why every field here
//! is `Option<bool>` and none carries a `#[serde(default)]`-to-`false`. On
//! the measured pinned build (`b1-10bf611`) the distinction never arises on
//! the wire: all nine keys are serialized verbatim with explicit
//! `true`/`false` on every config, including the no-template fallback. The
//! `Option` exists for the build where that stops holding.
//!
//! # The tri-state is never collapsed
//!
//! [`TemplateCapsState`] mirrors `BaselineState` in `gglib-proxy`'s `props`
//! module: "nobody has read it yet", "the read failed, and here is why", and
//! "here is what was read" are three different facts, and collapsing the
//! first two into the third's negative is exactly how unknown starts to gate
//! (ADR 0007, Consequences). [`reasoning_effort_support`] applies the same
//! rule one level down: a caps object whose field is absent answers
//! [`Support::Unknown`], never [`Support::No`].
//!
//! [ADR 0007]: https://github.com/mmogr/gglib/blob/main/docs/adr/0007-ask-the-server-for-template-capabilities.md

use serde::{Deserialize, Serialize};

/// The nine bools of `chat_template_caps`, as `GET /props` reports them.
///
/// Field names are byte-for-byte the wire keys measured on the pinned build
/// (`b1-10bf611`) — `template_caps_tests` pins the full list against a
/// fixture transcribed from that measurement, so an upstream rename or
/// addition fails loudly rather than silently reading as absent.
///
/// Every field is `Option<bool>`: `None` means the server did not report the
/// key, which — five defaults being `true` upstream — licenses no conclusion
/// in either direction. Unknown fields in the body are ignored (no
/// `deny_unknown_fields`): a future build adding a tenth cap must not make
/// the nine known ones unreadable.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TemplateCaps {
    /// Whether the template reads `tools`. Upstream default: `true`.
    pub supports_tools: Option<bool>,
    /// Whether the template renders assistant `tool_calls`. Upstream
    /// default: `true`.
    pub supports_tool_calls: Option<bool>,
    /// Whether the template accepts a `system` role. Upstream default:
    /// `true`.
    pub supports_system_role: Option<bool>,
    /// Whether multiple tool calls may appear in one assistant turn.
    /// Upstream default: `true`.
    pub supports_parallel_tool_calls: Option<bool>,
    /// Whether reasoning traces survive in the full history rather than only
    /// the last assistant message. Upstream default: `false`.
    pub supports_preserve_reasoning: Option<bool>,
    /// Whether the template **reads** the `reasoning_effort` variable (or its
    /// `reasoning_strength` alias — the probe binds both, `caps.cpp:29-32`).
    /// Upstream default: `false`.
    ///
    /// Read, not honoured: `stats.used` says the variable was accessed during
    /// an instrumented render, not that any particular level changes the
    /// output (ADR 0007 findings 2 and 3).
    pub supports_reasoning_effort: Option<bool>,
    /// Whether message content may be a plain string. Upstream default:
    /// `true`.
    pub supports_string_content: Option<bool>,
    /// Whether message content may be the typed parts array. Upstream
    /// default: `false`.
    pub supports_typed_content: Option<bool>,
    /// Whether tool-call arguments may be a JSON object rather than a
    /// string. Upstream default: `false`.
    pub supports_object_arguments: Option<bool>,
}

/// What gglib currently holds about a model's template caps.
///
/// Shaped like `BaselineState` in `gglib-proxy::props`, for the same reason:
/// an `Option<TemplateCaps>` flattens "nobody has read it yet" and "the read
/// was attempted and failed" into one `None`, after which the only thing a
/// surface can say is "not read yet" — a claim about a read that did happen.
/// ADR 0007's tri-state (supported / not supported / never observed) needs
/// the distinction held all the way down.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum TemplateCapsState {
    /// No `/props` read has completed for the running model yet. The
    /// ordinary state for the first second of a launch.
    #[default]
    NotYetRead,
    /// The read was attempted and produced no caps — the endpoint was
    /// unreachable, the body unparseable, or (a pre-caps build) the key
    /// absent.
    Unreadable {
        /// Cause, in words a dashboard can show.
        reason: String,
    },
    /// The server reported its template's caps.
    Read {
        /// The self-report, verbatim.
        caps: TemplateCaps,
    },
}

impl TemplateCapsState {
    /// The caps, when they were read.
    #[must_use]
    pub const fn caps(&self) -> Option<&TemplateCaps> {
        match self {
            Self::Read { caps } => Some(caps),
            Self::NotYetRead | Self::Unreadable { .. } => None,
        }
    }
}

/// One capability's answer, with unknown kept distinct from no.
///
/// The `ModelContext::catalog_resolved` discipline, applied to a self-report:
/// an observation that failed to arrive must not masquerade as one that
/// arrived negative (ADR 0007 decision 3 — unknown never gates).
/// `Deserialize` as well as `Serialize`, unlike the rest of this module: this
/// one crosses the HTTP boundary in both directions, because `ModelDetailDto`
/// carries it and the CLI reads that DTO back out of `--json`.
///
/// [`Default`] is [`Self::Unknown`], which is the only safe default there is —
/// a client omitting the field must never be read as a positive "no".
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum Support {
    /// The observed template positively reads the variable.
    Yes,
    /// The observed template positively does not read it.
    No,
    /// Never observed, or the observation did not carry this field.
    #[default]
    Unknown,
}

/// Whether a model's template reads `reasoning_effort`, from its recorded
/// caps.
///
/// [`Support::Unknown`] both when no caps were ever recorded (`None` — the
/// tri-state's "never observed") and when the recorded caps did not carry the
/// field. Only an explicit `false` answers [`Support::No`] — the arm ADR
/// 0007's suppression (a later PR) is allowed to act on.
#[must_use]
pub fn reasoning_effort_support(caps: &Option<TemplateCaps>) -> Support {
    match caps.as_ref().and_then(|c| c.supports_reasoning_effort) {
        Some(true) => Support::Yes,
        Some(false) => Support::No,
        None => Support::Unknown,
    }
}

#[cfg(test)]
#[path = "template_caps_tests.rs"]
mod template_caps_tests;
