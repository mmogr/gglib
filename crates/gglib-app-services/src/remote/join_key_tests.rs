//! Where a joining machine keeps the key it dials with, and what a join says
//! when the key there cannot be used.
//!
//! The refusals get past the local bind and stop at modelpipe's key check,
//! which comes before any endpoint exists, so they reach no other machine.
//! That the key lasts across joins takes a far machine to see, and is
//! `join_key_pipe_tests.rs`'s.

use std::fs;
use std::net::Ipv4Addr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use gglib_core::ports::AppEventEmitter;
use gglib_core::{RemotePairing, SettingsUpdate};
use tokio::net::TcpListener;

use super::super::types::JoinRequest;
use super::super::{RemoteGateway, RemoteOps};
use crate::error::GuiError;
use crate::test_support::test_core_and_proxy;
use crate::test_support_remote::{
    FINGERPRINT_A, KEY_A, RecordingEmitter, TICKET_A, TICKET_A_MOVED, TICKET_B, paired_with,
    scratch_join_keys, test_remote_ops_joining_from, ticket,
};

/// One machine is one file, named by its endpoint's fingerprint: the same
/// file wherever its ticket says it is, another for another machine, and
/// never the file this machine serves with.
#[tokio::test]
async fn a_machine_keeps_one_key_whatever_address_its_ticket_carries() {
    let root = gglib_core::paths::isolate_data_root();
    let dir = scratch_join_keys();
    let (_, ops, _) = test_remote_ops_joining_from(dir.clone()).await;

    let here = ops.join_key_path(&ticket(TICKET_A)).expect("named");
    let moved = ops.join_key_path(&ticket(TICKET_A_MOVED)).expect("named");
    let other = ops.join_key_path(&ticket(TICKET_B)).expect("named");

    assert_eq!(
        here,
        dir.join(FINGERPRINT_A),
        "the key is named by the far machine's fingerprint, in the directory it was given"
    );
    assert_eq!(
        moved, here,
        "the same machine at another address was given a second key"
    );
    assert_ne!(other, here, "two machines were given one key");
    // Where `remote_identity_path` puts the serving key, named here because
    // that function makes or tightens the data root's `data/`, which a test
    // that only names paths has no call to change.
    let serving = root.join("data").join("remote_identity");
    assert_ne!(here, serving, "a joining key is the serving key");
    assert_ne!(other, serving, "a joining key is the serving key");
}

/// The constructor a daemon is built with keeps the keys under the data
/// root, in `data/remote_join`.
///
/// The only `RemoteOps` this crate's tests build without
/// [`RemoteOps::with_join_keys`], so the only one that would see a daemon's
/// joins handed a key anywhere else. It dials nothing and only names the
/// path, under this binary's own data root (#955).
#[tokio::test]
async fn a_daemon_keeps_its_joining_keys_under_the_data_root() {
    let root = gglib_core::paths::isolate_data_root();
    let (core, proxy) = test_core_and_proxy().await;
    let emitter: Arc<dyn AppEventEmitter> = Arc::new(RecordingEmitter::default());
    let gateway = Arc::new(RemoteGateway::new(Arc::clone(&emitter)));
    let ops = RemoteOps::new(proxy, core, gateway, emitter, None);

    let key = ops.join_key_path(&ticket(TICKET_A)).expect("named");

    assert_eq!(
        key,
        root.join("data").join("remote_join").join(FINGERPRINT_A)
    );
}

/// What a refusal tells the person to do about a key path that has something
/// at it. Written out here, not taken from `identity.rs`, so that a change to
/// that sentence fails the tests that read it.
const KEPT_REMEDY: &str = "it was left as it is: fix it, or delete it and run \
     `gglib remote join` again for a new one, which that machine then sees as a new endpoint";

/// Machine A's key in `dir`, holding text that is not a key.
///
/// Written `0600` so that the contents are the only fault: modelpipe checks
/// who can read the file before it reads the key.
fn not_a_key(dir: &Path) -> PathBuf {
    fs::create_dir_all(dir).expect("the directory");
    let key = dir.join(FINGERPRINT_A);
    gglib_core::paths::create_private_file(&key).expect("an empty private file");
    fs::write(&key, "this is not a key\n").expect("written");
    key
}

/// A key file that holds something other than a key refuses the join, with
/// the file, modelpipe's reason and what to do, and is left as it was.
#[tokio::test]
async fn a_key_file_that_holds_no_key_refuses_the_join_and_is_kept() {
    let dir = scratch_join_keys();
    let (core, ops, events) = test_remote_ops_joining_from(dir.clone()).await;
    core.settings()
        .update(paired_with(TICKET_A, KEY_A))
        .await
        .expect("a pairing naming that machine admits a bare ticket for it");
    let key = not_a_key(&dir);

    let err = ops
        .join(JoinRequest {
            pairing: Some(TICKET_A.to_owned()),
            port: Some(0),
            discovery: false,
            ..JoinRequest::default()
        })
        .await
        .expect_err("a key that cannot be used is a join that does not go out");

    let GuiError::Internal(said) = err else {
        panic!("an unusable key is refused as the serving side's is: {err:?}");
    };
    assert!(said.contains(&key.display().to_string()), "{said}");
    assert!(said.contains("the identity file is not base32"), "{said}");
    assert!(said.contains(KEPT_REMEDY), "{said}");
    assert_eq!(
        fs::read_to_string(&key).expect("still there"),
        "this is not a key\n",
        "modelpipe read the file and refused it, and something replaced it"
    );
    assert!(events.events().is_empty(), "{:?}", events.events());
}

/// A key file anyone else can read refuses the join too, and modelpipe's
/// reason carries the remedy. The file holds a key, so its mode is the only
/// fault, and it is dialled with a code, so the refusal comes back through
/// `pair`.
#[cfg(unix)]
#[tokio::test]
async fn a_key_file_others_can_read_refuses_the_join_and_is_kept() {
    use std::os::unix::fs::PermissionsExt;

    let dir = scratch_join_keys();
    let (_, ops, events) = test_remote_ops_joining_from(dir.clone()).await;
    fs::create_dir_all(&dir).expect("the directory");
    let key = dir.join(FINGERPRINT_A);
    // Thirty-two zero bytes in the lower-case base32 modelpipe writes.
    let stored = format!("{}\n", "a".repeat(52));
    fs::write(&key, &stored).expect("written");
    fs::set_permissions(&key, fs::Permissions::from_mode(0o644)).expect("chmod");

    let err = ops
        .join(JoinRequest {
            pairing: Some(format!("{TICKET_A}-483920")),
            port: Some(0),
            discovery: false,
            ..JoinRequest::default()
        })
        .await
        .expect_err("a key others can read is a join that does not go out");

    let GuiError::Internal(said) = err else {
        panic!("an unusable key is refused as the serving side's is: {err:?}");
    };
    assert!(said.contains(&key.display().to_string()), "{said}");
    assert!(
        said.contains("is readable by others (mode 0644) — chmod 600 it"),
        "{said}"
    );
    let kept = fs::metadata(&key).expect("still there");
    assert_eq!(
        kept.permissions().mode() & 0o777,
        0o644,
        "the mode was changed"
    );
    assert_eq!(fs::read_to_string(&key).expect("read"), stored);
    assert!(events.events().is_empty(), "{:?}", events.events());
}

/// A symlink at the key's path refuses the join, with what to do about a
/// file that is there, and is left as it is.
///
/// modelpipe reads only a regular file. The link points at nothing, so a
/// look that followed it would find no file and tell the person to fix the
/// reason, when what is there to fix or delete is the link.
#[cfg(unix)]
#[tokio::test]
async fn a_symlink_at_the_key_path_refuses_the_join_and_is_kept() {
    let dir = scratch_join_keys();
    let (core, ops, events) = test_remote_ops_joining_from(dir.clone()).await;
    core.settings()
        .update(paired_with(TICKET_A, KEY_A))
        .await
        .expect("a pairing naming that machine admits a bare ticket for it");
    fs::create_dir_all(&dir).expect("the directory");
    let key = dir.join(FINGERPRINT_A);
    let nowhere = dir.join("nowhere");
    std::os::unix::fs::symlink(&nowhere, &key).expect("a link to nothing");

    let err = ops
        .join(JoinRequest {
            pairing: Some(TICKET_A.to_owned()),
            port: Some(0),
            discovery: false,
            ..JoinRequest::default()
        })
        .await
        .expect_err("a symlink at the key's path is a join that does not go out");

    let GuiError::Internal(said) = err else {
        panic!("an unusable key is refused as the serving side's is: {err:?}");
    };
    assert!(said.contains(&key.display().to_string()), "{said}");
    assert!(said.contains("is a symlink"), "{said}");
    assert!(said.contains(KEPT_REMEDY), "{said}");
    let kept = fs::symlink_metadata(&key).expect("still there");
    assert!(kept.file_type().is_symlink(), "the link was replaced");
    assert_eq!(fs::read_link(&key).expect("a link"), nowhere);
    assert!(
        fs::symlink_metadata(&nowhere).is_err(),
        "a key was written where the link points"
    );
    assert!(events.events().is_empty(), "{:?}", events.events());
}

/// A join whose remembered port is taken moves to a free one, and that
/// second dial is made from the same key: its refusal names the same file.
///
/// The first dial fails at the port, before modelpipe reads any key, so a
/// refusal of the key can only have come from the second.
#[tokio::test]
async fn a_join_that_moves_off_a_taken_port_dials_from_the_same_key() {
    let dir = scratch_join_keys();
    let (core, ops, _) = test_remote_ops_joining_from(dir.clone()).await;
    let taken = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        .await
        .expect("a free loopback port");
    let port = taken.local_addr().expect("the port just bound").port();
    core.settings()
        .update(SettingsUpdate {
            remote_pairing: Some(Some(RemotePairing {
                ticket: TICKET_A.to_owned(),
                api_key: KEY_A.to_owned(),
                default_model: None,
                port: Some(port),
                name: None,
            })),
            ..SettingsUpdate::default()
        })
        .await
        .expect("a pairing last reachable on the taken port");
    let key = not_a_key(&dir);

    let err = ops
        .join(JoinRequest {
            pairing: Some(TICKET_A.to_owned()),
            port: None,
            discovery: false,
            ..JoinRequest::default()
        })
        .await
        .expect_err("a key that cannot be used is a join that does not go out");

    let GuiError::Internal(said) = err else {
        panic!("the dial on a free port did not reach the key: {err:?}");
    };
    assert!(said.contains(&key.display().to_string()), "{said}");
    assert!(said.contains("the identity file is not base32"), "{said}");
}
