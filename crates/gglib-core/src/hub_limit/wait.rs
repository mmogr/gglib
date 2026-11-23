//! Waiting a limit out: what a client does at one ([`OnLimit`]), the wait
//! itself ([`wait_out`]) and the words it is shown in ([`waiting_sentence`]).

use std::fmt;
use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use tokio::time::Instant;

/// How often a wait tells its caller the time left.
const TICK: Duration = Duration::from_secs(1);

/// Told, while a client waits a limit out, the sentence to show: `Some` at
/// the start of the wait and once a second after, and `None` when the wait
/// is over, so a status line can be wiped.
pub type LimitNotice = Arc<dyn Fn(Option<&str>) + Send + Sync>;

/// What a Hub client does when the Hub refuses it, or its allowance is
/// already known to be spent.
#[derive(Clone, Default)]
pub enum OnLimit {
    /// Fail at once with the time the limit lifts. A daemon's client, and
    /// anything else that answers requests, does this: its caller decides
    /// whether to wait.
    #[default]
    Surface,
    /// Wait until the limit lifts, telling the notice, and ask again. A
    /// one-shot command's client does this.
    Wait(LimitNotice),
}

impl fmt::Debug for OnLimit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Surface => "Surface",
            Self::Wait(_) => "Wait",
        })
    }
}

/// `left` as minutes and seconds, `2:14`, its seconds rounded up so that a
/// wait never reads `0:00` before it is over.
#[must_use]
pub fn clock(left: Duration) -> String {
    let seconds = left.as_secs() + u64::from(left.subsec_nanos() > 0);
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

/// What every surface shows while a limit is waited out:
/// `waiting for Hugging Face's rate limit, 2:14`.
#[must_use]
pub fn waiting_sentence(left: Duration) -> String {
    format!("waiting for Hugging Face's rate limit, {}", clock(left))
}

/// Wait `left`, calling `on_tick` with the time still to wait at the start
/// and once a second after, until it has passed or `cancel` completes.
/// Whether `cancel` ended it.
///
/// `cancel` is polled first each time, so a wait cancelled as it ends is
/// cancelled.
pub async fn wait_out(
    left: Duration,
    cancel: impl Future<Output = ()>,
    mut on_tick: impl FnMut(Duration),
) -> bool {
    let end = Instant::now() + left;
    let mut cancel = std::pin::pin!(cancel);
    loop {
        let now = Instant::now();
        let remaining = end.saturating_duration_since(now);
        if remaining.is_zero() {
            return false;
        }
        on_tick(remaining);
        tokio::select! {
            biased;
            () = &mut cancel => return true,
            () = tokio::time::sleep_until(now + remaining.min(TICK)) => {}
        }
    }
}
