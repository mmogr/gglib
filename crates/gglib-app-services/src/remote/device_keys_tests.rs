//! The seed loop's per-row failure policy, against a real listener.
//!
//! The policy is a decision rather than an implementation detail, and it had
//! to be made explicitly: aborting on one bad row locks every device out for
//! the sake of one, and continuing silently drops a device while the roster
//! still lists it. The choice is to continue and log, which is why `list`
//! reports whether the listener is actually holding each row — the two stores
//! are allowed to disagree, and a person has to be able to see that they do.
//!
//! [`super::seed_into`] rather than [`super::seed`] on purpose: `seed` reads
//! the machine's own key file, which in a debug build is a real file in the
//! repository checkout, and the policy is separable from where the keys came
//! from.

use gglib_core::access::DeviceKeys;

use super::seed_into;

/// A listener that admits by name and reaches no network: no relay to find,
/// no discovery service to publish to, and no request ever sent through it.
async fn listener() -> modelpipe::ServeHandle {
    let backend = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("a loopback port to name as the backend");
    let addr = backend.local_addr().expect("the bound address");

    let mut opts = modelpipe::ServeOptions::default();
    opts.auth = modelpipe::TokenPolicy::Named;
    opts.discovery = false;
    opts.port_mapping = false;
    opts.allow_private_backend = true;
    modelpipe::serve(&format!("http://{addr}"), opts)
        .await
        .expect("the listener binds")
}

fn keys(rows: &[(&str, &str)]) -> DeviceKeys {
    rows.iter()
        .map(|(id, key)| ((*id).to_owned(), (*key).to_owned()))
        .collect()
}

/// One row the edge refuses does not cost the others theirs.
///
/// The refusable row here is an id outside modelpipe's name charset — what a
/// hand-edited key file produces. Aborting the arm for it would leave a
/// machine reachable by nobody, with the roster still listing every device.
#[tokio::test]
async fn a_row_the_edge_refuses_is_skipped_and_the_rest_are_seeded() {
    let handle = listener().await;

    let seeded = seed_into(
        &handle,
        keys(&[
            ("dev-0a1b2c3d", "sk-zzq-one"),
            ("not a valid name", "sk-zzq-two"),
            ("dev-11112222", "sk-zzq-three"),
        ]),
    );

    assert_eq!(seeded, 2, "the two well-formed rows were taken");
    let mut held = handle.token_names();
    held.sort();
    assert_eq!(
        held,
        vec!["dev-0a1b2c3d".to_owned(), "dev-11112222".to_owned()],
        "and the listener holds exactly those"
    );

    handle.shutdown().await;
}

/// A machine that has never invited anything seeds nothing and says nothing
/// is wrong. Under `Named` that is a listener admitting only a live grant,
/// which is the intended state after the clean break.
#[tokio::test]
async fn an_empty_roster_seeds_nothing() {
    let handle = listener().await;

    assert_eq!(seed_into(&handle, DeviceKeys::new()), 0);
    assert!(handle.token_names().is_empty());

    handle.shutdown().await;
}
