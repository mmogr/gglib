//! Tests for [`super::await_shutdown`].
//!
//! Split out via `#[path]`, as this repo's other test modules are.

use super::*;

/// The signal path must cancel the token, not merely observe it.
///
/// Without this, everything bounded by the token — `/api/events` above all —
/// never ends, so `with_graceful_shutdown` never returns and `perform_shutdown`
/// never runs. Ctrl-C, `systemctl stop` and `kill` all take this path, so the
/// bug hid behind the one trigger that did work: the API route, which cancels
/// the token itself.
#[tokio::test]
async fn the_signal_path_cancels_the_token() {
    let token = CancellationToken::new();
    await_shutdown(std::future::ready(()), token.clone()).await;
    assert!(token.is_cancelled());
}

/// The API path already cancels the token before this future is polled; firing
/// it again must be harmless, which is what lets the cancel be unconditional.
#[tokio::test]
async fn the_api_path_tolerates_a_second_cancel() {
    let token = CancellationToken::new();
    token.cancel();
    await_shutdown(std::future::pending(), token.clone()).await;
    assert!(token.is_cancelled());
}
