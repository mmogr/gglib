//! A status read names the paired machine by its stored name, and leaves the
//! data root as it was: it names the endpoint key and reads the device keys
//! without creating `data/` or tightening its mode.
//!
//! The data root is this binary's test root (`isolate_data_root`), one per
//! process and shared by every test in it, so another test can make `data/`
//! there at any moment. So the read runs in a child: the parent re-runs this
//! binary for this one test, whose fresh root nothing else touches, and the
//! child reads status there with a `RemoteOps` built the way the daemon builds
//! it, with no key file of its own. The child is chosen by name and by a
//! variable only the parent sets, so an ordinary run of it checks nothing, and
//! the parent fails on a missing marker if a rename leaves it unrun.

use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;

use gglib_core::domain::UNNAMED_PAIRED;
use gglib_core::ports::AppEventEmitter;

use super::super::{RemoteGateway, RemoteOps};
use crate::test_support::test_core_and_proxy;
use crate::test_support_remote::{
    FINGERPRINT_A, KEY_A, RecordingEmitter, TICKET_A, paired_with, test_remote_ops,
};

/// The child's path in this test binary.
const CHILD: &str = "remote::status::status_tests::the_child_reads_status_over_a_fresh_root";

/// Set by the parent for the child alone.
const CHILD_ENV: &str = "GGLIB_STATUS_READ_CHILD";

/// Printed by the child once every check below it has passed.
const MARKER: &str = "STATUS-READ-LEFT-THE-DATA-ROOT-ALONE";

#[test]
fn a_status_read_creates_nothing_and_tightens_nothing() {
    let out = Command::new(std::env::current_exe().expect("this test binary"))
        .args(["--exact", CHILD, "--nocapture", "--test-threads=1"])
        .env(CHILD_ENV, "1")
        .output()
        .expect("re-run this binary for the child test");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);

    assert!(
        out.status.success(),
        "the child's status read changed the data root:\n{stdout}\n{stderr}"
    );
    assert!(
        stdout.contains(MARKER),
        "the child never reported its checks:\n{stdout}\n{stderr}"
    );
}

/// Run by the parent above, alone in its process, so its test root is fresh.
#[tokio::test]
async fn the_child_reads_status_over_a_fresh_root() {
    if std::env::var(CHILD_ENV).is_err() {
        println!("not the parent's child run: nothing to check");
        return;
    }
    let root = PathBuf::from(gglib_core::paths::isolate_data_root());
    let data = root.join("data");
    assert!(!data.exists(), "the child's test root is not fresh");

    let (core, proxy) = test_core_and_proxy().await;
    let emitter: Arc<dyn AppEventEmitter> = Arc::new(RecordingEmitter::default());
    let gateway = Arc::new(RemoteGateway::new(Arc::clone(&emitter)));
    let ops = RemoteOps::new(proxy, core, gateway, emitter, None);
    assert!(!data.exists(), "building the ops made data/");

    let status = ops.status().await;
    assert!(!data.exists(), "a status read made data/");
    assert_eq!(
        status.identity_path,
        Some(data.join("remote_identity").display().to_string()),
        "status names the endpoint key's file"
    );

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::create_dir(&data).expect("make data/");
        std::fs::set_permissions(&data, std::fs::Permissions::from_mode(0o755))
            .expect("open data/ to the group and others");
        ops.status().await;
        let mode = std::fs::metadata(&data)
            .expect("data/")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o755, "a status read tightened data/ to {mode:o}");
    }

    println!("{MARKER}");
}

/// The status names the paired machine by the name its stored pairing holds,
/// beside the fingerprint it is compared by, and by nothing when none is
/// held: a surface then shows its own words, never the fingerprint.
#[tokio::test]
async fn the_status_names_the_paired_machine_by_its_stored_name() {
    let (core, ops, _) = test_remote_ops().await;
    let mut pairing = paired_with(TICKET_A, KEY_A);
    core.settings()
        .update(pairing.clone())
        .await
        .expect("machine A's pairing is stored");
    let unnamed = ops.status().await;
    assert_eq!(unnamed.paired_name, None);
    assert_eq!(unnamed.paired_shown(), UNNAMED_PAIRED);

    if let Some(Some(stored)) = pairing.remote_pairing.as_mut() {
        stored.name = Some("desk".to_owned());
    }
    core.settings()
        .update(pairing)
        .await
        .expect("machine A's name is stored");
    let named = ops.status().await;
    assert_eq!(named.paired_name.as_deref(), Some("desk"));
    assert_eq!(named.paired_shown(), "desk");
    assert_eq!(
        named.stored_ticket_fingerprint.as_deref(),
        Some(FINGERPRINT_A)
    );
}
