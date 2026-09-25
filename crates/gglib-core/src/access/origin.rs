//! Whether a page in a browser may change something on this machine.

use crate::cors::CorsConfig;

/// Whether a request that changes something may proceed, judged by what the
/// browser that sent it says about where it came from.
///
/// An endpoint that asks no credential on loopback trusts whatever can open
/// its socket, and a browser opens it for any page it has loaded. CORS
/// decides whether that page may read the answer; it does not stop the
/// request being sent, and a form post or a `no-cors` fetch is sent without
/// asking first. The caller asks this for every method but `GET`, `HEAD` and
/// `OPTIONS`.
///
/// With an `Origin`:
///
/// - One naming the host the request was sent to (`http://` or `https://`,
///   then exactly the `Host` header, ignoring case) is the endpoint's own
///   page, and passes. That is sound only behind a Host guard: a page on a
///   rebound name also names the host it was sent to, and the guard is what
///   refuses that name.
/// - Any other passes when `cors` lets it read
///   ([`CorsConfig::allows_origin`]), so a page that names any origin but
///   the endpoint's own may change something here exactly when the CORS
///   layer lets it read the answer.
/// - `null`, an origin without `://`, and one whose scheme or authority is
///   empty (`://host`, `http://`) are refused whatever `cors` says: a page
///   that hides where it came from cannot be judged. The caller passes an
///   `Origin` that is not text as `""`.
///
/// With no `Origin` the request is a program's, which sends none, unless its
/// fetch metadata says `Sec-Fetch-Site: cross-site`. Fetch metadata is not
/// read when an `Origin` is present: the desktop app and the Vite dev server
/// are cross-site by it, and their origins are the ones `cors` admits.
#[must_use]
pub fn may_change(
    cors: &CorsConfig,
    origin: Option<&str>,
    fetch_site: Option<&str>,
    host: &str,
) -> bool {
    let Some(origin) = origin else {
        return !fetch_site.is_some_and(|site| site.trim().eq_ignore_ascii_case("cross-site"));
    };
    let Some((scheme, authority)) = origin.split_once("://") else {
        return false;
    };
    if scheme.is_empty() || authority.is_empty() {
        return false;
    }
    let same_origin = matches!(scheme, "http" | "https") && authority.eq_ignore_ascii_case(host);
    same_origin || cors.allows_origin(origin)
}

#[cfg(test)]
#[path = "origin_tests.rs"]
mod origin_tests;
