//! The one way this workspace builds an HTTP client for a server on this
//! machine.
//!
//! gglib talks to itself constantly: the proxy forwards every request to
//! llama-server, the runtime polls its health and reads its `/props`, the CLI
//! and the desktop app ask the daemon on `127.0.0.1:9887`, the benchmark posts
//! prompts to a server it started, and `--remote` reaches the far machine
//! through a pipe whose near end is a local port. Every one of those is
//! `http://127.0.0.1:<port>/…`.
//!
//! A `reqwest::Client` built with defaults honours `HTTP_PROXY`, `ALL_PROXY`
//! and, with hyper-util's `client-proxy-system` feature (turned on by the
//! workspace's reqwest `system-proxy` feature, so present in every binary gglib
//! ships), the operating system's own proxy settings. Its matcher skips only
//! the hosts named in `NO_PROXY`; loopback is not special-cased. So on a
//! machine with a proxy in the environment, such a client asks gglib's own
//! daemon through somebody else's machine ([#1085]): a fresh llama-server
//! never reads as healthy, the proxy forwards prompts to the proxy host, and
//! `gglib` on the command line reports its own daemon's port as held by
//! another program.
//!
//! [`client_builder`] is a `reqwest::ClientBuilder` with `no_proxy()` already
//! applied; a caller adds its own timeouts. [`client`] is the `no_proxy()` twin
//! of `reqwest::Client::new()`. The clients that fetch from the internet, in
//! `gglib-download` and `gglib-hf`, are deliberately not built here: they must
//! honour a proxy.
//!
//! `no_proxy()` is also what keeps a client's first request fast on macOS and
//! Windows. Without it `build()` pushes `ProxyMatcher::system()`, which on
//! macOS opens an `SCDynamicStore` and copies the system proxies
//! synchronously: about half a second in a warm process, against
//! microseconds with `no_proxy()` ([#1084]). On Linux the matcher reads
//! environment variables only, which is cheap.
//!
//! Two CLI commands take the proxy's host from the user (`proxy dashboard` and
//! `proxy cache-clear`, default `127.0.0.1`, and a proxy started with
//! `--share-lan` is reachable from elsewhere). [`client_for`] gives them the
//! loopback client when the host names this machine and reqwest's default,
//! which honours a proxy, when it does not.
//!
//! Two tests hold this in place. `gglib-runtime`'s `health_proxy_tests` runs
//! two health checks built from this builder in a child process whose proxy
//! variables point at a recorder, and asserts the recorder saw neither.
//! `loopback_tests` here reads the sources of the crates that talk to this
//! machine and fails on any `reqwest` client built anywhere but here.
//!
//! [#1084]: https://github.com/mmogr/gglib/issues/1084
//! [#1085]: https://github.com/mmogr/gglib/issues/1085

/// A client builder that will never route through a proxy.
///
/// For a server on this machine, with the caller's own timeouts added.
pub fn client_builder() -> reqwest::ClientBuilder {
    reqwest::Client::builder().no_proxy()
}

/// The `no_proxy()` twin of `reqwest::Client::new()`: a client with reqwest's
/// defaults that goes straight to the port in the URL.
///
/// # Panics
///
/// Where `reqwest::Client::new()` panics: if a TLS backend cannot be
/// initialised. Callers that would rather report that use [`client_builder`].
#[must_use]
pub fn client() -> reqwest::Client {
    client_builder()
        .build()
        .expect("a loopback client builds wherever reqwest::Client::new() would")
}

/// A client for the host the user named: [`client`] when it is this machine,
/// reqwest's default, which honours a proxy, when it is not.
///
/// # Panics
///
/// Where `reqwest::Client::new()` panics.
#[must_use]
pub fn client_for(host: &str) -> reqwest::Client {
    if is_this_machine(host) {
        client()
    } else {
        reqwest::Client::new()
    }
}

/// Whether `host` names this machine: `localhost` in any case, with or without
/// a trailing dot, or a loopback address, IPv4-mapped IPv6 included, with or
/// without the brackets an IPv6 host carries in a URL. Decided from the
/// spelling, without resolving a name; the dotted-quad shorthand `127.1` is
/// not one of the spellings, so type the address out.
#[must_use]
pub fn is_this_machine(host: &str) -> bool {
    let bare = host
        .trim_start_matches('[')
        .trim_end_matches(']')
        .trim_end_matches('.');
    bare.eq_ignore_ascii_case("localhost")
        || bare.parse::<std::net::IpAddr>().is_ok_and(|ip| match ip {
            std::net::IpAddr::V4(v4) => v4.is_loopback(),
            std::net::IpAddr::V6(v6) => {
                v6.is_loopback() || v6.to_ipv4_mapped().is_some_and(|v4| v4.is_loopback())
            }
        })
}

#[cfg(test)]
#[path = "loopback_scanner.rs"]
mod loopback_scanner;

#[cfg(test)]
#[path = "loopback_tests.rs"]
mod loopback_tests;
