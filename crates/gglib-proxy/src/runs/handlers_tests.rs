//! What `?tools=` says, read as the route reads its query: `none`, or the
//! ids of servers in canonical decimal, and nothing else.

use std::collections::BTreeSet;

use axum::extract::Query;
use axum::http::Uri;

use super::PutQuery;

/// `PUT /v1/runs/p1` with `query`, read as the route reads it.
fn read(query: &str) -> Result<PutQuery, String> {
    let uri: Uri = format!("/v1/runs/p1?{query}").parse().unwrap();
    Query::<PutQuery>::try_from_uri(&uri)
        .map(|Query(query)| query)
        .map_err(|e| e.body_text())
}

/// The ids of the servers `query`'s `tools` names.
fn servers(query: &str) -> BTreeSet<i64> {
    read(query).unwrap().tools.expect("a tools value").0
}

#[test]
fn tools_reads_none_as_no_server() {
    assert_eq!(servers("tools=none"), BTreeSet::new());
    assert!(servers("kind=chat&tools=none&draw=true").is_empty());
    assert!(read("kind=chat").unwrap().tools.is_none(), "absent");
}

#[test]
fn tools_reads_ids_sorted_and_once() {
    let three_seven = BTreeSet::from([3, 7]);
    for query in ["tools=3,7", "tools=7,3", "tools=7,3,7", "tools=3%2C7"] {
        assert_eq!(servers(query), three_seven, "{query}");
    }
    assert_eq!(servers("tools=3"), BTreeSet::from([3]));
    let largest = format!("tools={}", i64::MAX);
    assert_eq!(servers(&largest), BTreeSet::from([i64::MAX]));
}

#[test]
fn tools_refuses_builtin_empty_and_malformed_lists() {
    let too_large = format!("tools={}0", i64::MAX);
    for query in [
        "tools=builtin",
        "tools=mcp",
        "tools=off",
        "tools=",
        "tools=3,,7",
        "tools=3,",
        "tools=,3",
        "tools=0",
        "tools=-1",
        "tools=%2B3",
        "tools=+3",
        "tools=03",
        "tools=a",
        "tools=3a",
        "tools=none,3",
        "tools=3%20",
        "tools=1&tools=2",
        too_large.as_str(),
    ] {
        assert!(read(query).is_err(), "{query} was read");
    }
}
