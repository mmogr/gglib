//! The HTTP client every health check of a server on this machine uses.
//!
//! The checks themselves are in `crate::process::health`: one that waits for
//! a server to come up, and one single-shot probe the request fast path and
//! [`crate::health_monitor`] share.

use reqwest::Client;
use std::time::Duration;

/// A client for asking a server on this machine's loopback address how it is.
///
/// [`gglib_proxy::loopback::client_builder`] with a two-second timeout: that
/// builder never routes through a proxy, and [`gglib_proxy::loopback`] says
/// why. `health_proxy_tests` runs the single-shot check, on its own and
/// through the monitor, with a proxy in the environment and watches where the
/// requests land; `wait_for_http_health` is covered by sharing this
/// constructor.
pub(crate) fn loopback_client() -> reqwest::Result<Client> {
    gglib_proxy::loopback::client_builder()
        .timeout(Duration::from_secs(2))
        .build()
}

#[cfg(test)]
#[path = "health_proxy_tests.rs"]
pub(crate) mod health_proxy_tests;
