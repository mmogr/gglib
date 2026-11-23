//! A `RateLimit` header read as the Hub writes it, and a refusal read with
//! and without one.

use std::time::Duration;

use super::*;

fn limit(bucket: HubBucket, remaining: u64, seconds: u64) -> HubLimit {
    HubLimit {
        bucket,
        remaining,
        resets_in: Duration::from_secs(seconds),
    }
}

/// The headers huggingface.co sent on 2026-11-23: a refusal of the API, a
/// file read beside it, and an ordinary answer of the API.
#[test]
fn the_hubs_recorded_headers_are_read() {
    assert_eq!(
        parse("\"api\";r=0;t=10"),
        Some(limit(HubBucket::Api, 0, 10))
    );
    assert_eq!(
        parse("\"resolvers\";r=2999;t=9"),
        Some(limit(HubBucket::Resolvers, 2999, 9))
    );
    assert_eq!(
        parse("\"api\";r=485;t=142"),
        Some(limit(HubBucket::Api, 485, 142))
    );
    assert_eq!(
        parse("\"pages\";r=99;t=1"),
        Some(limit(HubBucket::Pages, 99, 1))
    );
}

/// Spaces around the parts, the parameters in another order and one gglib
/// does not read do not change what is read.
#[test]
fn spacing_order_and_other_parameters_are_tolerated() {
    for text in [
        "\"api\";t=10;r=0",
        " \"api\" ; r = 0 ; t = 10 ",
        "\"api\";pk=:abc:;r=0;t=10",
        "\"api\";r=0;t=10;",
        "\"api\";flag;r=0;t=10",
    ] {
        assert_eq!(parse(text), Some(limit(HubBucket::Api, 0, 10)), "{text}");
    }
}

/// Anything else is no limit: the policy header, a name gglib does not know
/// or one not quoted, a count missing or not a number.
#[test]
fn anything_else_is_no_limit() {
    for text in [
        "\"fixed window\";\"api\";q=500;w=300",
        "\"downloads\";r=0;t=10",
        "api;r=0;t=10",
        "\"api\";r=0",
        "\"api\";t=10",
        "\"api\";r=-1;t=10",
        "\"api\";r=0;t=soon",
        "",
        "garbage",
    ] {
        assert_eq!(parse(text), None, "{text}");
    }
}

/// The recorded refusal is read from its header, whatever its address.
#[test]
fn a_refusal_with_a_header_is_read_from_it() {
    let read = refusal(
        Some("\"api\";r=0;t=10"),
        Some("10"),
        "/owner/repo/resolve/main/README.md",
    );

    assert_eq!(read, limit(HubBucket::Api, 0, 10));
}

/// A refusal leaves none, whatever its header says.
#[test]
fn a_refusal_leaves_none() {
    let read = refusal(Some("\"api\";r=3;t=10"), None, "/api/models");

    assert_eq!(read.remaining, 0);
}

/// Without a usable header a refusal waits its `Retry-After`, on the
/// allowance of the address asked.
#[test]
fn a_refusal_without_a_header_waits_its_retry_after() {
    let api = refusal(None, Some("9"), "/api/models/owner/repo/tree/main");
    let file = refusal(
        Some("garbage"),
        Some(" 7 "),
        "/owner/repo/resolve/main/x.gguf",
    );

    assert_eq!(api, limit(HubBucket::Api, 0, 9));
    assert_eq!(file, limit(HubBucket::Resolvers, 0, 7));
}

/// With neither, it waits a minute.
#[test]
fn a_refusal_that_names_no_time_waits_a_minute() {
    let read = refusal(None, Some("Wed, 21 Oct 2026 07:28:00 GMT"), "/api/models");

    assert_eq!(read, limit(HubBucket::Api, 0, 60));
    assert_eq!(read.resets_in, HEADERLESS_WAIT);
}

/// An address is read by its segments: `resolve` is a file read, a first
/// segment `api` the API, and anything else a page.
#[test]
fn an_address_names_its_allowance() {
    assert_eq!(
        HubBucket::of_path("/owner/repo/resolve/main/x.gguf"),
        HubBucket::Resolvers
    );
    assert_eq!(HubBucket::of_path("/api/models/owner/repo"), HubBucket::Api);
    assert_eq!(HubBucket::of_path("api/models"), HubBucket::Api);
    assert_eq!(HubBucket::of_path("/owner/repo"), HubBucket::Pages);
    assert_eq!(HubBucket::of_path("/owner/apiary"), HubBucket::Pages);
    assert_eq!(HubBucket::of_path("/owner/resolver/x"), HubBucket::Pages);
}
