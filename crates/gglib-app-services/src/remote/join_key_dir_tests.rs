//! The directory a joining key goes in, made at every join, and what a join
//! says when that directory, or the first key in it, cannot be made.
//!
//! Every join here stops before any endpoint exists: at a port that is
//! taken, at the directory, or at modelpipe's key check. None reaches
//! another machine.

use std::fs;
use std::net::Ipv4Addr;
use std::path::Path;
use std::sync::Arc;

use tokio::net::TcpListener;

use super::super::RemoteOps;
use super::super::types::JoinRequest;
use crate::error::GuiError;
use crate::test_support_remote::{
    KEY_A, TICKET_A, paired_with, scratch_join_keys, test_remote_ops_joining_from,
};

/// A `RemoteOps` that keeps its join keys in `dir` and holds a pairing with
/// machine A, so a bare ticket for A is dialled.
async fn paired_with_a(dir: &Path) -> Arc<RemoteOps> {
    let (core, ops, _) = test_remote_ops_joining_from(dir.to_path_buf()).await;
    core.settings()
        .update(paired_with(TICKET_A, KEY_A))
        .await
        .expect("a pairing naming that machine admits a bare ticket for it");
    ops
}

/// A join for machine A from the stored pairing, on `port`.
fn join_a(port: u16) -> JoinRequest {
    JoinRequest {
        pairing: Some(TICKET_A.to_owned()),
        port: Some(port),
        discovery: false,
        ..JoinRequest::default()
    }
}

/// Every join makes the directory its key goes in, not only the first: one
/// removed between two joins is there again at the second.
///
/// `--port` pins a port that is taken, so each join is refused at the bind,
/// which comes after the directory is made and before modelpipe reads or
/// mints a key. The directory is left empty, and nothing is dialled.
#[tokio::test]
async fn every_join_makes_the_directory_its_key_goes_in() {
    let dir = scratch_join_keys();
    let ops = paired_with_a(&dir).await;
    let taken = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        .await
        .expect("a free loopback port");
    let port = taken.local_addr().expect("the port just bound").port();

    let first = ops.join(join_a(port)).await;
    let made = fs::metadata(&dir).map(|made| made.is_dir());
    let emptied = fs::remove_dir(&dir);
    let second = ops.join(join_a(port)).await;
    let remade = fs::metadata(&dir);

    let bound = format!("could not bind 127.0.0.1:{port}");
    for refused in [&first, &second] {
        assert!(
            matches!(refused, Err(GuiError::Conflict(said)) if said.contains(&bound)),
            "{refused:?}"
        );
    }
    assert!(made.expect("the first join made no directory"));
    emptied.expect("the first join's directory was not empty");
    let remade = remade.expect("the second join made no directory");
    assert!(remade.is_dir());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(remade.permissions().mode() & 0o777, 0o700);
    }
}

/// A join whose key directory cannot be made is refused before it dials,
/// with the directory, the reason and what to do, and the file in the way is
/// left as it was.
#[tokio::test]
async fn a_key_directory_that_cannot_be_made_refuses_the_join() {
    let scratch = scratch_join_keys();
    fs::create_dir_all(&scratch).expect("a scratch directory");
    let in_the_way = scratch.join("a-file");
    fs::write(&in_the_way, "not a directory\n").expect("written");
    let dir = in_the_way.join("remote_join");
    // What making that directory says, from the call gglib's own makes.
    let cause = fs::create_dir_all(&dir)
        .expect_err("no directory can be made under a file")
        .to_string();
    let ops = paired_with_a(&dir).await;

    let err = ops
        .join(join_a(0))
        .await
        .expect_err("a directory that cannot be made is a join that does not go out");

    let GuiError::Internal(said) = err else {
        panic!("a key directory that cannot be made is refused as a key is: {err:?}");
    };
    let named = format!(
        "could not make {}, where the key this machine joins with is kept: {cause}",
        dir.display()
    );
    assert!(said.contains(&named), "{said}");
    assert!(said.contains("move aside any file in its way"), "{said}");
    assert!(said.contains("run `gglib remote join` again"), "{said}");
    assert_eq!(
        fs::read_to_string(&in_the_way).expect("still there"),
        "not a directory\n"
    );
}

/// A first key that cannot be written refuses the join with modelpipe's
/// reason, and sends nobody to delete a file that is not there.
///
/// The directory is `0500`, the owner's to read and not to write, which
/// `create_private_dir` leaves as it is: it takes away only what group and
/// others have. A user no mode binds, such as root, would mint the key and
/// dial, so the test first shows that this one cannot write there, and
/// makes no join if it can.
#[cfg(unix)]
#[tokio::test]
async fn a_first_key_that_cannot_be_written_refuses_the_join_with_nothing_to_delete() {
    use std::os::unix::fs::PermissionsExt;

    use crate::test_support_remote::FINGERPRINT_A;

    let dir = scratch_join_keys();
    fs::create_dir_all(&dir).expect("the directory");
    fs::set_permissions(&dir, fs::Permissions::from_mode(0o500)).expect("chmod 500");
    let unwritable = fs::File::create(dir.join("probe")).map(drop);
    let ops = paired_with_a(&dir).await;

    let joined = match unwritable {
        Err(_) => Some(ops.join(join_a(0)).await),
        Ok(()) => None,
    };
    fs::set_permissions(&dir, fs::Permissions::from_mode(0o700)).expect("chmod 700");

    let cause = unwritable
        .expect_err("this user wrote into a directory with no write bit, so no key write can fail")
        .to_string();
    let err = joined
        .expect("joined")
        .expect_err("a key that cannot be written is a join that does not go out");
    let GuiError::Internal(said) = err else {
        panic!("a key that cannot be written is refused as one that cannot be used: {err:?}");
    };
    let key = dir.join(FINGERPRINT_A);
    assert!(said.contains(&key.display().to_string()), "{said}");
    assert!(said.contains(&cause), "{said}");
    assert!(said.contains("gglib finds no file at that path"), "{said}");
    assert!(said.contains("run `gglib remote join` again"), "{said}");
    assert!(!said.contains("delete"), "{said}");
    assert!(
        fs::symlink_metadata(&key).is_err(),
        "a key was written after all"
    );
}
