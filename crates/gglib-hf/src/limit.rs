//! The Hub's rate limit as this client meets it.
//!
//! The `RateLimit` header of every answer the client reads goes into its
//! [`HubGate`] (a redirect reqwest follows is not read, so its header is
//! not), and a request is not sent while the allowance it counts against is
//! closed.
//! At a closed allowance, or a refusal (429), the client follows its
//! [`OnLimit`]: it fails at once with [`HfError::RateLimited`], or it waits
//! the limit out, telling its notice, and sends again. A refusal is not a
//! failure the 5xx and network retries count: it is answered here, and
//! never reaches them.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use gglib_core::hub_limit::{
    HubBucket, HubGate, LIMITED_IN_A_ROW, LimitNotice, OnLimit, parse, refusal, wait_out,
    waiting_sentence,
};
use reqwest::{RequestBuilder, Response, StatusCode};
use tokio::time::Instant;
use url::Url;

use crate::error::{HfError, HfResult};

/// The gate a client asks before it sends, and what it does at a limit.
#[derive(Default)]
pub(crate) struct HubLimits {
    gate: Arc<HubGate>,
    on_limit: OnLimit,
    /// How many of this client's requests are waiting a limit out now, so
    /// the notice is told the wait is over only when the last one ends.
    waiting: AtomicUsize,
}

impl HubLimits {
    /// Limits kept in `gate`, met as `on_limit` says.
    pub(crate) fn new(gate: Arc<HubGate>, on_limit: OnLimit) -> Self {
        Self {
            gate,
            on_limit,
            waiting: AtomicUsize::new(0),
        }
    }

    /// Send the request `build` makes for `url`, built afresh for each
    /// attempt, until an answer that is not a refusal comes, and note the
    /// limit each answer states.
    ///
    /// The outer error is the limit, when this client fails at one or has
    /// been refused [`LIMITED_IN_A_ROW`] times; the inner result is the
    /// send's own, for the caller's retries to judge.
    pub(crate) async fn send(
        &self,
        url: &Url,
        build: &(impl Fn() -> RequestBuilder + Sync),
    ) -> HfResult<reqwest::Result<Response>> {
        let mut bucket = HubBucket::of_path(url.path());
        let mut refused = 0;
        loop {
            self.admit(bucket).await?;
            let response = match build().send().await {
                Ok(response) => response,
                unsent @ Err(_) => return Ok(unsent),
            };
            let now = Instant::now();
            let stated = header(&response, "ratelimit");
            if response.status() != StatusCode::TOO_MANY_REQUESTS {
                if let Some(limit) = stated.and_then(parse) {
                    self.gate.note(limit, now);
                }
                return Ok(Ok(response));
            }
            let limit = refusal(stated, header(&response, "retry-after"), url.path());
            self.gate.note(limit, now);
            bucket = limit.bucket;
            refused += 1;
            if matches!(self.on_limit, OnLimit::Surface) || refused >= LIMITED_IN_A_ROW {
                return Err(HfError::RateLimited {
                    bucket,
                    resets_in: self.gate.closed(bucket, now).unwrap_or(limit.resets_in),
                });
            }
        }
    }

    /// Return once `bucket` is open: at once when it is, else after the
    /// wait, or with the limit when this client fails at one.
    async fn admit(&self, bucket: HubBucket) -> HfResult<()> {
        let Some(left) = self.gate.closed(bucket, Instant::now()) else {
            return Ok(());
        };
        match &self.on_limit {
            OnLimit::Surface => Err(HfError::RateLimited {
                bucket,
                resets_in: left,
            }),
            OnLimit::Wait(notice) => {
                self.wait(left, notice).await;
                Ok(())
            }
        }
    }

    /// Wait `left`, telling `notice` the sentence each second, and that the
    /// wait is over when no other request of this client is still waiting.
    async fn wait(&self, left: Duration, notice: &LimitNotice) {
        let _waiting = Waiting::start(&self.waiting, notice);
        // Nothing here cancels the wait: a command ends it by ending.
        wait_out(left, std::future::pending(), |left| {
            notice(Some(&waiting_sentence(left)));
        })
        .await;
    }
}

/// One request's place among those waiting. The last to leave, however it
/// leaves, tells the notice the wait is over.
struct Waiting<'a> {
    count: &'a AtomicUsize,
    notice: &'a LimitNotice,
}

impl<'a> Waiting<'a> {
    fn start(count: &'a AtomicUsize, notice: &'a LimitNotice) -> Self {
        count.fetch_add(1, Ordering::SeqCst);
        Self { count, notice }
    }
}

impl Drop for Waiting<'_> {
    fn drop(&mut self) {
        if self.count.fetch_sub(1, Ordering::SeqCst) == 1 {
            (self.notice)(None);
        }
    }
}

/// The text of `response`'s `name` header, when it has one that is text.
fn header<'a>(response: &'a Response, name: &str) -> Option<&'a str> {
    response.headers().get(name)?.to_str().ok()
}

#[cfg(test)]
#[path = "limit_tests.rs"]
mod tests;
