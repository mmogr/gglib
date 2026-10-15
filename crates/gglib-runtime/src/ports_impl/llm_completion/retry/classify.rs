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
//! [`describe`] prints both, the `type` first; that ordering settles what
//! gets printed, never what gets retried.
//!
//! The text a reason copies from the body is cut, and any image data in it
//! is replaced, before it is printed ([`clip`]). That reads the text only to
//! render it: nothing found there reaches the verdict.

use std::time::Duration;

use chrono::{DateTime, Utc};
use gglib_core::ports::model_runtime::is_retryable_error_type;
use gglib_proxy::models::{ErrorDetail, ErrorResponse};
use reqwest::{Response, StatusCode};

use super::headers::parse_retry_after;

/// Upper bound on how much of an error body's text is kept for the message.
/// Enough to diagnose, short enough not to flood a log line.
const MAX_BODY_CHARS: usize = 500;

/// What image data in a body's text is replaced with.
const IMAGE_DATA_REMOVED: &str = "<image data removed>";

/// The shortest unbroken base64 run taken for image data. Longer than a
/// digest a message might name (a SHA-256 is 64 hex or 44 base64
/// characters); a `data:` URL is removed whatever its length.
const MIN_BASE64_RUN: usize = 100;

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
            format!("{status}: {}", clip(&body)),
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
/// The label is every discriminant the body carries: this proxy's `type`
/// with its `code` after it in brackets, since the `type` is one of a few
/// classes and the `code` is what names the refusal; or a modelpipe
/// refusal's `code` alone, which is all it has. Both can be missing — an
/// absent `type` deserializes as the empty string — and the empty label is
/// dropped rather than printed, so a body that names neither reads as
/// `500 Internal Server Error: boom` instead of growing a stray space before
/// the colon. The message is [`clip`]ped like an unrecognised body.
fn describe(status: StatusCode, error: &ErrorDetail) -> String {
    let kind = error.r#type.as_str();
    let code = error.code.as_deref().unwrap_or_default();
    let label = match (kind.is_empty(), code.is_empty() || code == kind) {
        (true, _) => code.to_owned(),
        (false, true) => kind.to_owned(),
        (false, false) => format!("{kind} ({code})"),
    };
    let message = clip(&error.message);
    if label.is_empty() {
        format!("{status}: {message}")
    } else {
        format!("{status} {label}: {message}")
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

/// A body's text as a reason may print it: image data replaced, then cut to
/// a length that is safe to log.
///
/// A request carries each image as a `data:` URL, and an upstream that
/// wrote any of the request back into its error would put the image in the
/// log, the retry notice and the run's error event. The replacing comes
/// first, so the cut cannot leave part of an image behind.
fn clip(text: &str) -> String {
    let text = replace_spans(text, data_url_at);
    let text = replace_spans(&text, base64_run_at);
    truncate(&text)
}

/// `text` with each span that `span_at` calls image data replaced by
/// [`IMAGE_DATA_REMOVED`].
///
/// `span_at` is asked at a byte offset where the span ends, and whether it
/// is image data. Every span it reports as image data starts and ends on an
/// ASCII byte or the end of the text, so the slices below are on character
/// boundaries.
fn replace_spans(text: &str, span_at: fn(&[u8], usize) -> (usize, bool)) -> String {
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let (mut copied, mut at) = (0, 0);
    while at < bytes.len() {
        let (end, image) = span_at(bytes, at);
        if image {
            out.push_str(&text[copied..at]);
            out.push_str(IMAGE_DATA_REMOVED);
            copied = end;
        }
        at = end;
    }
    out.push_str(&text[copied..]);
    out
}

/// A `data:` URL starting at `at`: the scheme, case aside, then everything
/// up to the first byte no URL in a message carries, holding the comma every
/// data URL has before its payload.
fn data_url_at(bytes: &[u8], at: usize) -> (usize, bool) {
    let scheme = b"data:";
    let rest = &bytes[at..];
    if !rest
        .get(..scheme.len())
        .is_some_and(|head| head.eq_ignore_ascii_case(scheme))
    {
        return (at + 1, false);
    }
    let len = rest.iter().position(|&b| ends_url(b)).unwrap_or(rest.len());
    if rest[..len].contains(&b',') {
        (at + len, true)
    } else {
        (at + 1, false)
    }
}

/// Whether `b` ends a URL copied into a message: whitespace, a quote, a
/// bracket, or any byte outside ASCII.
const fn ends_url(b: u8) -> bool {
    !b.is_ascii()
        || b.is_ascii_whitespace()
        || matches!(
            b,
            b'"' | b'\'' | b'`' | b'<' | b'>' | b'(' | b')' | b'[' | b']' | b'{' | b'}'
        )
}

/// An unbroken run of the standard base64 alphabet starting at `at`, which
/// is image data when it is at least [`MIN_BASE64_RUN`] long.
fn base64_run_at(bytes: &[u8], at: usize) -> (usize, bool) {
    let is_base64 = |b: &u8| b.is_ascii_alphanumeric() || matches!(b, b'+' | b'/' | b'=');
    let rest = &bytes[at..];
    match rest
        .iter()
        .position(|b| !is_base64(b))
        .unwrap_or(rest.len())
    {
        0 => (at + 1, false),
        len => (at + len, len >= MIN_BASE64_RUN),
    }
}

/// Cut text to [`MAX_BODY_CHARS`].
fn truncate(body: &str) -> String {
    if body.chars().count() <= MAX_BODY_CHARS {
        return body.to_owned();
    }
    let kept: String = body.chars().take(MAX_BODY_CHARS).collect();
    format!("{kept}… <truncated>")
}
