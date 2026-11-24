//! A transfer at Hugging Face's rate limit.
//!
//! Every answer's `RateLimit` header goes into the gate the transfer was
//! handed, which the process's Hub client keeps too. A connection is not
//! opened while the allowance the file counts against is closed, and a
//! refusal (429) is waited out and the file asked for again. Either wait
//! shows [`waiting_sentence`] on the row once a second, and a cancel ends
//! it. A refusal spends none of the reconnects a lost connection does, and
//! keeps what the last connection handed on.

use reqwest::{Response, StatusCode, Url};
use tokio::time::Instant;

use super::native::{NativeDownload, NativeError, notify};
use gglib_core::hub_limit::{
    HubBucket, HubGate, HubLimit, LIMITED_IN_A_ROW, parse, refusal, wait_out, waiting_sentence,
};

/// The refusals a download has had in a row with nothing arriving between.
#[derive(Default)]
pub(super) struct Limited {
    /// Refusals in a row.
    in_a_row: u32,
    /// Bytes the download had received at the last of them.
    received: u64,
}

impl Limited {
    /// Return once the allowance `req`'s file counts against is open: at
    /// once when `gate` holds it open, else after waiting until it opens.
    pub(super) async fn admit(req: &NativeDownload<'_>, gate: &HubGate) -> Result<(), NativeError> {
        // An address that does not parse is refused when it is sent.
        let Ok(url) = Url::parse(req.url) else {
            return Ok(());
        };
        match gate.closed(HubBucket::of_path(url.path()), Instant::now()) {
            Some(left) => wait(req, left).await,
            None => Ok(()),
        }
    }

    /// The Hub refused the download with `limit`, having received `received`
    /// bytes before. Count it, and wait until the allowance opens; or, at
    /// [`LIMITED_IN_A_ROW`] in a row, end the download with
    /// [`NativeError::KeptLimited`].
    ///
    /// Bytes received since the last refusal start the count again.
    pub(super) async fn after(
        &mut self,
        req: &NativeDownload<'_>,
        gate: &HubGate,
        limit: HubLimit,
        received: u64,
    ) -> Result<(), NativeError> {
        if received > self.received {
            *self = Self {
                in_a_row: 0,
                received,
            };
        }
        self.in_a_row += 1;
        tracing::warn!(
            url = req.url,
            bucket = %limit.bucket,
            refused = self.in_a_row,
            "Hugging Face's rate limit refused the download"
        );
        if self.in_a_row >= LIMITED_IN_A_ROW {
            return Err(NativeError::KeptLimited);
        }
        let left = gate
            .closed(limit.bucket, Instant::now())
            .unwrap_or(limit.resets_in);
        wait(req, left).await
    }
}

/// Note the limit `response` states in `gate`; the limit it refused the
/// request with, when it is a refusal (429).
///
/// A refusal that names no allowance is filed under the one a request for
/// `asked`, the path the download asked for, counts against, whichever hop
/// of its redirects answered it: a CDN's path says nothing of the Hub's.
pub(super) fn heard(gate: &HubGate, response: &Response, asked: &str) -> Option<HubLimit> {
    let now = Instant::now();
    let stated = header(response, "ratelimit");
    if response.status() != StatusCode::TOO_MANY_REQUESTS {
        if let Some(limit) = stated.and_then(parse) {
            gate.note(limit, now);
        }
        return None;
    }
    let limit = refusal(stated, header(response, "retry-after"), asked);
    gate.note(limit, now);
    Some(limit)
}

/// Wait `left`, showing on the row the time still to wait each second, or
/// end in [`NativeError::Cancelled`] when the download is cancelled first.
async fn wait(req: &NativeDownload<'_>, left: std::time::Duration) -> Result<(), NativeError> {
    let cancel = async {
        match req.cancel.as_ref() {
            Some(cancel) => cancel.cancelled().await,
            None => std::future::pending().await,
        }
    };
    let cancelled = wait_out(left, cancel, |left| {
        notify(req, &waiting_sentence(left));
    })
    .await;
    if cancelled {
        Err(NativeError::Cancelled)
    } else {
        Ok(())
    }
}

/// The text of `response`'s `name` header, when it has one that is text.
fn header<'a>(response: &'a Response, name: &str) -> Option<&'a str> {
    response.headers().get(name)?.to_str().ok()
}

#[cfg(test)]
#[path = "native_limit_tests.rs"]
mod tests;
