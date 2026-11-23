//! A daemon call Hugging Face's rate limit refused, sent again once it
//! lifts.
//!
//! The daemon answers a call whose Hub read the limit refused at once, with
//! 429, the type `hub_rate_limited` and the seconds until the limit lifts:
//! it does not hold the request while it waits. A call a one-shot command
//! makes on the user's behalf waits them out, its status line counting
//! down, and is sent again: it is sent at most [`LIMITED_IN_A_ROW`] times
//! in all, so it waits at most one time fewer. Any other answer, a 429 of
//! another type among them, is the call's own.

use std::time::Duration;

use anyhow::Result;
use gglib_core::hub_limit::{
    LIMITED_IN_A_ROW, LimitNotice, MAX_RESET, MIN_CLOSED, wait_out, waiting_sentence,
};

use super::{DaemonHandle, paths};

impl DaemonHandle {
    /// Send the request `build` makes, and again, built afresh, each time
    /// the daemon answers that Hugging Face's rate limit refused it, once
    /// the time it names has passed, telling `notice` the wait each second
    /// and that it is over.
    ///
    /// The answer is the first that is not such a refusal, for the caller
    /// to read. The [`LIMITED_IN_A_ROW`]th refusal in a row is an error, as
    /// [`Self::refusal`] reads it, and so is a 429 of any other type.
    pub(super) async fn sent_past_the_hubs_limit(
        build: impl Fn() -> reqwest::RequestBuilder,
        notice: &LimitNotice,
    ) -> Result<reqwest::Response> {
        let mut refused = 0;
        loop {
            let response = build().send().await?;
            let status = response.status();
            if status != reqwest::StatusCode::TOO_MANY_REQUESTS {
                return Ok(response);
            }
            let body = response.text().await.unwrap_or_default();
            refused += 1;
            let Some(left) = hub_limited(&body).filter(|_| refused < LIMITED_IN_A_ROW) else {
                return Err(Self::refusal(status, body));
            };
            let _waiting = Waiting(notice);
            wait_out(left, std::future::pending(), |left| {
                notice(Some(&waiting_sentence(left)));
            })
            .await;
        }
    }
}

/// A wait on the status line, which is told the wait is over however it
/// ends: a call dropped mid-wait leaves no line drawn.
struct Waiting<'a>(&'a LimitNotice);

impl Drop for Waiting<'_> {
    fn drop(&mut self) {
        (self.0)(None);
    }
}

/// The time a `hub_rate_limited` refusal says the limit lifts in, held
/// between [`MIN_CLOSED`] and [`MAX_RESET`] as a Hub client holds a reset;
/// `None` for any other body.
fn hub_limited(body: &str) -> Option<Duration> {
    let value: serde_json::Value = serde_json::from_str(body).ok()?;
    if value.get("type")?.as_str()? != paths::HUB_LIMITED_TYPE {
        return None;
    }
    let seconds = value.get("metadata")?.get("retry_after_secs")?.as_u64()?;
    Some(Duration::from_secs(seconds).clamp(MIN_CLOSED, MAX_RESET))
}

#[cfg(test)]
#[path = "hub_limited_tests.rs"]
mod tests;
