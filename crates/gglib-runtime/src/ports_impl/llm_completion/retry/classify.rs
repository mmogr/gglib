//! Response classification: retryable, terminal, or success.
//!
//! Two structured signals, in order of authority, and no inspection of
//! human-readable message text at any point:
//!
//! 1. **The error body.** When the upstream is the gglib proxy it sends
//!    [`ErrorResponse`], whose `type` discriminant is resolved through
//!    [`is_retryable_error_type`] — the same predicate the IPC surface uses, so
//!    HTTP and IPC cannot disagree about what is worth retrying. A new
//!    retryable [`ModelRuntimeError`](gglib_core::ports::ModelRuntimeError)
//!    variant therefore needs no change here.
//! 2. **The HTTP status.** When the adapter points straight at a llama-server
//!    rather than the proxy, the body is not ours to interpret, so
//!    classification falls back to status semantics alone.

use std::time::Duration;

use chrono::{DateTime, Utc};
use gglib_core::ports::model_runtime::is_retryable_error_type;
use gglib_proxy::models::ErrorResponse;
use reqwest::{Response, StatusCode};

use super::headers::parse_retry_after;

/// Upper bound on how much of an unrecognised error body is kept for the
/// message. Enough to diagnose, short enough not to flood a log line.
const MAX_BODY_CHARS: usize = 500;

/// A failed attempt, and whether it is worth another one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Failure {
    /// The condition is transient; the same request may succeed if repeated.
    Retryable {
        /// The upstream's own `Retry-After`, when it supplied a usable one.
        retry_after: Option<Duration>,
        /// Human-readable cause, for logs and the user-facing retry notice.
        reason: String,
    },
    /// Repeating the request would fail the same way.
    Terminal {
        /// Human-readable cause.
        reason: String,
    },
}

impl Failure {
    /// The cause, whichever variant this is.
    pub(super) fn reason(&self) -> &str {
        match self {
            Self::Retryable { reason, .. } | Self::Terminal { reason } => reason,
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

    let (retryable, reason) = match serde_json::from_str::<ErrorResponse>(&body) {
        Ok(err) => (
            is_retryable_error_type(&err.error.r#type),
            format!("{status} {}: {}", err.error.r#type, err.error.message),
        ),
        Err(_) => (
            status_is_retryable(status),
            format!("{status}: {}", truncate(&body)),
        ),
    };

    Err(if retryable {
        Failure::Retryable {
            retry_after,
            reason,
        }
    } else {
        Failure::Terminal { reason }
    })
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
