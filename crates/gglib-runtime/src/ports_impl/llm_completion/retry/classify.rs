//! Response classification: retryable, terminal, or success.
//!
//! Three structured signals, and no inspection of human-readable message text
//! at any point:
//!
//! 1. **The error body's `type`.** When the upstream is the gglib proxy it sends
//!    [`ErrorResponse`], whose `type` discriminant is resolved through
//!    [`is_retryable_error_type`] — the same predicate the IPC surface uses, so
//!    HTTP and IPC cannot disagree about what is worth retrying. A new
//!    retryable [`ModelRuntimeError`](gglib_core::ports::ModelRuntimeError)
//!    variant therefore needs no change here.
//! 2. **The error body's `code`,** for the one author that writes this shape
//!    without a `type`: the modelpipe edge in front of a remote machine. See
//!    [`code_is_retryable`] for why the status alone cannot stand in for it.
//! 3. **The HTTP status.** When the adapter points straight at a llama-server
//!    rather than the proxy, the body is not ours to interpret, so
//!    classification falls back to status semantics alone.
//!
//! Only the third is ranked. The first two are read as a disjunction, not in
//! precedence order — [`error_is_retryable`] says what that costs — and the
//! status decides alone whenever the body offers neither, whether because it
//! is not this shape at all or because its author filled in neither field.
//! Authority *is* ordered in [`describe`], which picks one of the two as the
//! label; that ordering settles what gets printed, never what gets retried.

use std::time::Duration;

use chrono::{DateTime, Utc};
use gglib_core::ports::model_runtime::is_retryable_error_type;
use gglib_proxy::models::{ErrorDetail, ErrorResponse};
use reqwest::{Response, StatusCode};

use super::headers::parse_retry_after;

/// Upper bound on how much of an unrecognised error body is kept for the
/// message. Enough to diagnose, short enough not to flood a log line.
const MAX_BODY_CHARS: usize = 500;

/// modelpipe's `error.code` for a connect side that has no tunnel right now.
const TUNNEL_UNAVAILABLE: &str = "tunnel_unavailable";

/// A failed attempt, and whether it is worth another one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Failure {
    /// The condition is transient; the same request may succeed if repeated.
    Retryable {
        /// The upstream's own `Retry-After`, when it supplied a usable one.
        retry_after: Option<Duration>,
        /// Human-readable cause, for logs and the user-facing retry notice.
        reason: String,
        /// The body's own `code` — see [`Failure::code`].
        code: Option<String>,
    },
    /// Repeating the request would fail the same way.
    Terminal {
        /// Human-readable cause.
        reason: String,
        /// The body's own `code` — see [`Failure::code`].
        code: Option<String>,
    },
}

impl Failure {
    /// The cause, whichever variant this is.
    pub(super) fn reason(&self) -> &str {
        match self {
            Self::Retryable { reason, .. } | Self::Terminal { reason, .. } => reason,
        }
    }

    /// The upstream's own machine-readable `code`, when its body wrote one.
    ///
    /// Carried out beside [`reason`](Self::reason) rather than recovered from
    /// it. A caller that wants to *act* on one specific condition — rather
    /// than merely report it — needs the term the body's author wrote, and
    /// the contract these module docs open with forbids reading it back out
    /// of the rendered sentence. [`describe`] folds the discriminant, the
    /// status and the message into one string precisely so a person can read
    /// it; re-parsing that string is exactly the text matching ruled out
    /// above.
    ///
    /// Both variants carry it because it is a fact about the body, not about
    /// the verdict: the same parse produces it either way. Only the terminal
    /// path reads it today, and putting it on that variant alone would encode
    /// the current reader rather than the shape of the data.
    pub(super) fn code(&self) -> Option<&str> {
        match self {
            Self::Retryable { code, .. } | Self::Terminal { code, .. } => code.as_deref(),
        }
    }
}

/// Sort a response into success or a classified failure.
///
/// A successful response is returned untouched so the caller can hand it to the
/// stream decoder — nothing is read from its body here.
pub(super) async fn classify(response: Response, now: DateTime<Utc>) -> Result<Response, Failure> {
    if response.status().is_success() {
        return Ok(response);
    }

    // Headers must be read before the body consumes the response.
    let status = response.status();
    let retry_after = parse_retry_after(response.headers(), now);
    let body = response.text().await.unwrap_or_default();

    // A body that is not this shape at all has no `code` to carry: whatever
    // the truncated text holds, nothing structured was parsed out of it, and
    // inventing one from the status would be the text matching these docs rule
    // out wearing a different hat.
    let (retryable, reason, code) = match serde_json::from_str::<ErrorResponse>(&body) {
        Ok(err) => (
            error_is_retryable(status, &err.error),
            describe(status, &err.error),
            err.error.code,
        ),
        Err(_) => (
            status_is_retryable(status),
            format!("{status}: {}", truncate(&body)),
            None,
        ),
    };

    Err(if retryable {
        Failure::Retryable {
            retry_after,
            reason,
            code,
        }
    } else {
        Failure::Terminal { reason, code }
    })
}

/// Retryability of a body in the [`ErrorResponse`] shape, whichever
/// discriminant its author actually filled in.
///
/// A body that fills in neither says nothing about retrying, so the status
/// decides — the same answer such a body got before `type` was defaulted,
/// when it failed to deserialize and reached [`status_is_retryable`] by
/// falling out of the parse. Parsing it must not change the verdict: doing so
/// would have made a bare `{"error":{"message":…}}` at 503 terminal.
///
/// The two discriminants are read as a disjunction rather than in precedence
/// order: they name different things — `type` is a class of condition, `code`
/// is a specific one — and neither vocabulary contains a term the other would
/// classify differently. That is a fact about today's two vocabularies, not a
/// rule the code enforces: a body carrying a terminal `type` *and* a retryable
/// `code` would be called retryable by the `code`. So adding to
/// [`code_is_retryable`] a code this proxy also emits means revisiting the
/// disjunction, not just that list.
fn error_is_retryable(status: StatusCode, error: &ErrorDetail) -> bool {
    if error.r#type.is_empty() && error.code.is_none() {
        return status_is_retryable(status);
    }
    is_retryable_error_type(&error.r#type) || error.code.as_deref().is_some_and(code_is_retryable)
}

/// Whether a modelpipe `error.code` denotes a condition worth waiting out.
///
/// Only `tunnel_unavailable`, and it needs naming here because the status
/// fallback cannot reach it: modelpipe answers all three of its 502s with the
/// same status, so retryability is carried entirely by the code. This one is
/// written by the *connect* side when `keep_connected` is between tunnels —
/// a laptop that moved from wifi to a hotspot, which is the case that loop
/// exists for and which heals within seconds.
///
/// The other two 502s are deliberately excluded. `bad_gateway` and
/// `backend_unreachable` are written by the serving side about the model
/// server behind it — stopped, on another port, or wedged — and none of those
/// is a condition a repeated request waits out.
fn code_is_retryable(code: &str) -> bool {
    code == TUNNEL_UNAVAILABLE
}

/// Render a structured error body the way someone reading a log needs it.
///
/// The label is whichever discriminant the body carries: this proxy's `type`,
/// or a modelpipe refusal's `code`, which is all it has. Both can be missing —
/// an absent `type` deserializes as the empty string — and the empty label is
/// dropped rather than printed, so a body that names neither reads as
/// `500 Internal Server Error: boom` instead of growing a stray space before
/// the colon.
fn describe(status: StatusCode, error: &ErrorDetail) -> String {
    let label = if error.r#type.is_empty() {
        error.code.as_deref().unwrap_or_default()
    } else {
        error.r#type.as_str()
    };
    if label.is_empty() {
        format!("{status}: {}", error.message)
    } else {
        format!("{status} {label}: {}", error.message)
    }
}

/// Status-only retryability, for upstreams that are not the gglib proxy.
///
/// Both of these are defined by RFC 9110 as conditions the client is expected
/// to wait out, and both are the statuses that carry `Retry-After`.
fn status_is_retryable(status: StatusCode) -> bool {
    matches!(
        status,
        StatusCode::SERVICE_UNAVAILABLE | StatusCode::TOO_MANY_REQUESTS
    )
}

/// Clip an unrecognised body to a length that is safe to log.
fn truncate(body: &str) -> String {
    if body.chars().count() <= MAX_BODY_CHARS {
        return body.to_owned();
    }
    let kept: String = body.chars().take(MAX_BODY_CHARS).collect();
    format!("{kept}… <truncated>")
}
