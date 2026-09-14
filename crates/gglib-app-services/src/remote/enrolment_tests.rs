//! Tests for what `invite`'s two-store write takes back when it fails.
//!
//! A `#[path]` sibling of `enrolment.rs`.

use super::*;
use crate::test_support_remote::test_remote_ops;

/// #1034: a roster write that fails takes its key back out of the file before
/// the roster lock is let go, so a `list` or `status` queued behind it never
/// reads a key with no row.
///
/// Settings validation refuses the row, because it will not store an id
/// modelpipe could not hold, and the key file takes any id.
#[tokio::test]
async fn a_roster_write_that_fails_takes_its_key_back_out_of_the_file() {
    let (_, ops, _) = test_remote_ops().await;

    let refused = remember(&ops, "dev 1111", "sk-zzq-taken-back").await;

    assert!(refused.is_err(), "validation refuses the row: {refused:?}");
    assert!(
        read_keys(&ops).expect("the key file reads").is_empty(),
        "the key is out of the file before anything else can read it"
    );
    assert!(
        ops.list().await.expect("list").is_empty(),
        "and nothing is listed"
    );
}
