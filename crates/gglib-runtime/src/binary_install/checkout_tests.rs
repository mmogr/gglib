//! The source checkout, against a local repository with tagged releases.

use std::path::{Path, PathBuf};
use std::process::Command;

use tokio::sync::mpsc;

use super::{Checkout, SourceRepo, clone_args, leftovers, prepare_checkout, retire_checkout};
use crate::binary_install::ReleaseSelector;
use crate::llama::BuildEvent;

/// Run git in `dir` with a throwaway identity and none of the caller's
/// repository variables or signing; panic on failure.
fn git(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE")
        .args([
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@example.com",
            "-c",
            "commit.gpgsign=false",
            "-c",
            "tag.gpgsign=false",
        ])
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .expect("git runs");
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}

/// An upstream with two releases, `b1` and `b2`, each one commit, and the
/// `v1` alias on `b1` that llama.cpp's `v*` releases are; the `file://` URL
/// a shallow clone of it needs.
fn upstream(root: &Path) -> String {
    let up = upstream_dir(root);
    std::fs::create_dir_all(&up).unwrap();
    git(&up, &["init", "-q", "-b", "main"]);
    std::fs::write(up.join("release.txt"), "one").unwrap();
    git(&up, &["add", "release.txt"]);
    git(&up, &["commit", "-q", "-m", "one"]);
    git(&up, &["tag", "b1"]);
    git(&up, &["tag", "v1"]);
    std::fs::write(up.join("release.txt"), "two").unwrap();
    git(&up, &["commit", "-q", "-am", "two"]);
    git(&up, &["tag", "b2"]);
    format!("file://{}", up.display())
}

fn upstream_dir(root: &Path) -> PathBuf {
    root.join("upstream")
}

fn repo(url: &str) -> SourceRepo<'_> {
    SourceRepo {
        product: "thing",
        url,
        submodules: false,
    }
}

fn tag(name: &str) -> ReleaseSelector {
    ReleaseSelector::Tag(name.to_owned())
}

/// Prepare, collecting what was logged.
fn prepare(
    url: &str,
    selector: &ReleaseSelector,
    home: &Path,
) -> (anyhow::Result<Checkout>, Vec<String>) {
    let (tx, mut rx) = mpsc::channel(256);
    let checkout = prepare_checkout(&repo(url), selector, home, &tx);
    let logs = std::iter::from_fn(|| rx.try_recv().ok())
        .filter_map(|event| match event {
            BuildEvent::Log { message } => Some(message),
            _ => None,
        })
        .collect();
    (checkout, logs)
}

/// A checkout of `release` at `home`, settled, as a finished build leaves it.
fn installed(url: &str, release: &str, home: &Path) {
    prepare(url, &tag(release), home)
        .0
        .expect("clones")
        .settle()
        .expect("settles");
}

fn release_in(dir: &Path) -> String {
    std::fs::read_to_string(dir.join("release.txt")).unwrap()
}

/// Where a replacement is cloned while the old checkout stays.
fn staging(home: &Path) -> PathBuf {
    leftovers(home)[0].clone()
}

#[test]
fn a_clone_is_shallow_at_the_tag_with_submodules_only_when_asked() {
    let dir = PathBuf::from("/x/.llama/llama.cpp");
    let llama = SourceRepo {
        product: "llama.cpp",
        url: "https://github.com/ggml-org/llama.cpp",
        submodules: false,
    };
    assert_eq!(
        clone_args(&llama, &tag("b11429"), &dir),
        [
            "clone",
            "--depth",
            "1",
            "--branch",
            "b11429",
            "https://github.com/ggml-org/llama.cpp",
            "/x/.llama/llama.cpp",
        ]
    );
    let sd = SourceRepo {
        submodules: true,
        ..llama
    };
    assert_eq!(
        clone_args(&sd, &ReleaseSelector::Latest, &dir),
        [
            "clone",
            "--depth",
            "1",
            "--recurse-submodules",
            "--shallow-submodules",
            "https://github.com/ggml-org/llama.cpp",
            "/x/.llama/llama.cpp",
        ]
    );
}

#[test]
fn what_a_checkout_leaves_beside_it_is_named_after_it() {
    assert_eq!(
        leftovers(Path::new("/x/.llama/llama.cpp")),
        [
            PathBuf::from("/x/.llama/llama.cpp.new"),
            PathBuf::from("/x/.llama/llama.cpp.old"),
        ]
    );
}

#[test]
fn a_first_build_clones_the_tag_in_place() {
    let tmp = tempfile::tempdir().unwrap();
    let url = upstream(tmp.path());
    let home = tmp.path().join("src");

    let (checkout, _) = prepare(&url, &tag("b1"), &home);
    let checkout = checkout.expect("clones");

    assert_eq!(checkout.dir, home);
    assert_eq!(checkout.version, "b1");
    assert_eq!(checkout.tag.as_deref(), Some("b1"));
    assert_eq!(
        checkout.commit_sha,
        git(&upstream_dir(tmp.path()), &["rev-parse", "b1"])
    );
    assert!(checkout.commit_sha.starts_with(&checkout.short_sha));
    assert_eq!(release_in(&home), "one");
    checkout.settle().expect("nothing to move");
    assert_eq!(release_in(&home), "one");
}

#[test]
fn a_checkout_already_at_the_tag_is_reused_with_its_build() {
    let tmp = tempfile::tempdir().unwrap();
    let url = upstream(tmp.path());
    let home = tmp.path().join("src");
    installed(&url, "b1", &home);
    std::fs::create_dir_all(home.join("build")).unwrap();
    std::fs::write(home.join("build/marker"), "kept").unwrap();

    let (checkout, logs) = prepare(&url, &tag("b1"), &home);
    let checkout = checkout.expect("reuses");

    assert_eq!(checkout.dir, home);
    assert_eq!(logs, ["Using the existing thing checkout at b1."]);
    assert!(home.join("build/marker").exists());
}

/// llama.cpp's `v*` release points at the commit of the `b<n>` it names, so
/// a checkout at one is at the other.
#[test]
fn any_tag_at_the_checkouts_commit_counts() {
    let tmp = tempfile::tempdir().unwrap();
    let url = upstream(tmp.path());
    let home = tmp.path().join("src");
    installed(&url, "b1", &home);

    let checkout = prepare(&url, &tag("v1"), &home).0.expect("reuses");

    assert_eq!(checkout.dir, home);
    assert_eq!(checkout.version, "v1");
}

#[test]
fn a_checkout_at_another_release_is_replaced_only_once_settled() {
    let tmp = tempfile::tempdir().unwrap();
    let url = upstream(tmp.path());
    let home = tmp.path().join("src");
    installed(&url, "b1", &home);
    std::fs::create_dir_all(home.join("build")).unwrap();
    std::fs::write(home.join("build/marker"), "old").unwrap();

    let checkout = prepare(&url, &tag("b2"), &home).0.expect("clones b2");

    // The old checkout is untouched while the new one builds.
    assert_eq!(checkout.dir, staging(&home));
    assert_eq!(release_in(&home), "one");
    assert!(home.join("build/marker").exists());
    assert_eq!(release_in(&checkout.dir), "two");
    assert_eq!(checkout.version, "b2");

    checkout.settle().expect("moves into place");
    assert_eq!(release_in(&home), "two");
    assert!(!home.join("build/marker").exists());
    for left in leftovers(&home) {
        assert!(!left.exists(), "{} is left", left.display());
    }
}

/// `CMake` records the folder it configured, and refuses a build directory
/// that has moved; a settled checkout starts its next build afresh.
#[test]
fn a_replacement_does_not_carry_its_build_directory_into_place() {
    let tmp = tempfile::tempdir().unwrap();
    let url = upstream(tmp.path());
    let home = tmp.path().join("src");
    installed(&url, "b1", &home);
    let checkout = prepare(&url, &tag("b2"), &home).0.expect("clones b2");
    std::fs::create_dir_all(checkout.dir.join("build")).unwrap();
    std::fs::write(checkout.dir.join("build/CMakeCache.txt"), "staged").unwrap();

    checkout.settle().expect("moves into place");

    assert!(!home.join("build").exists());
    let again = prepare(&url, &tag("b2"), &home).0.expect("reuses");
    assert_eq!(again.dir, home);
}

#[test]
fn a_checkout_with_changes_to_a_tracked_file_is_refused() {
    let tmp = tempfile::tempdir().unwrap();
    let url = upstream(tmp.path());
    let home = tmp.path().join("src");
    installed(&url, "b1", &home);
    std::fs::write(home.join("release.txt"), "patched").unwrap();

    let err = prepare(&url, &tag("b2"), &home).0.expect_err("refused");

    let message = format!("{err:#}");
    assert!(message.contains(&home.display().to_string()), "{message}");
    assert!(message.contains("changes of its own"), "{message}");
    assert_eq!(release_in(&home), "patched");
    assert!(!staging(&home).exists());
}

#[test]
fn a_checkout_with_a_commit_of_its_own_is_refused() {
    let tmp = tempfile::tempdir().unwrap();
    let url = upstream(tmp.path());
    let home = tmp.path().join("src");
    installed(&url, "b1", &home);
    git(&home, &["commit", "-q", "--allow-empty", "-m", "mine"]);

    prepare(&url, &tag("b2"), &home).0.expect_err("refused");

    assert!(!staging(&home).exists());
}

#[test]
fn a_commit_on_a_local_branch_away_from_head_is_refused() {
    let tmp = tempfile::tempdir().unwrap();
    let url = upstream(tmp.path());
    let home = tmp.path().join("src");
    installed(&url, "b1", &home);
    git(&home, &["checkout", "-q", "-b", "mine"]);
    git(&home, &["commit", "-q", "--allow-empty", "-m", "mine"]);
    git(&home, &["checkout", "-q", "--detach", "b1"]);

    prepare(&url, &tag("b2"), &home).0.expect_err("refused");

    assert!(!staging(&home).exists());
}

#[test]
fn a_checkout_with_a_stash_is_refused() {
    let tmp = tempfile::tempdir().unwrap();
    let url = upstream(tmp.path());
    let home = tmp.path().join("src");
    installed(&url, "b1", &home);
    std::fs::write(home.join("release.txt"), "patched").unwrap();
    git(&home, &["stash", "-q"]);
    assert_eq!(release_in(&home), "one");

    prepare(&url, &tag("b2"), &home).0.expect_err("refused");

    assert!(!staging(&home).exists());
}

#[test]
fn an_untracked_file_does_not_stop_a_replacement() {
    let tmp = tempfile::tempdir().unwrap();
    let url = upstream(tmp.path());
    let home = tmp.path().join("src");
    installed(&url, "b1", &home);
    std::fs::write(home.join("notes.txt"), "mine").unwrap();

    let checkout = prepare(&url, &tag("b2"), &home).0.expect("clones b2");

    assert_eq!(checkout.dir, staging(&home));
}

/// A build takes long enough for someone to start work in the old checkout;
/// what they did is not deleted under them.
#[test]
fn work_that_appears_during_the_build_keeps_the_old_checkout() {
    let tmp = tempfile::tempdir().unwrap();
    let url = upstream(tmp.path());
    let home = tmp.path().join("src");
    installed(&url, "b1", &home);
    let checkout = prepare(&url, &tag("b2"), &home).0.expect("clones b2");
    std::fs::write(home.join("release.txt"), "patched").unwrap();

    let err = checkout.settle().expect_err("refused");

    let message = format!("{err:#}");
    assert!(message.contains(&home.display().to_string()), "{message}");
    assert_eq!(release_in(&home), "patched");
    assert_eq!(release_in(&staging(&home)), "two");
}

#[test]
fn an_old_checkout_left_by_an_unfinished_settle_is_cleared_first() {
    let tmp = tempfile::tempdir().unwrap();
    let url = upstream(tmp.path());
    let home = tmp.path().join("src");
    installed(&url, "b1", &home);
    let retired = leftovers(&home)[1].clone();
    std::fs::create_dir_all(&retired).unwrap();
    std::fs::write(retired.join("left-over"), "x").unwrap();

    installed(&url, "b2", &home);

    assert_eq!(release_in(&home), "two");
    assert!(!retired.exists());
}

#[test]
fn a_new_checkout_left_by_an_unfinished_attempt_is_replaced() {
    let tmp = tempfile::tempdir().unwrap();
    let url = upstream(tmp.path());
    let home = tmp.path().join("src");
    installed(&url, "b1", &home);
    let left = staging(&home);
    std::fs::create_dir_all(&left).unwrap();
    std::fs::write(left.join("left-over"), "x").unwrap();

    let checkout = prepare(&url, &tag("b2"), &home).0.expect("clones b2");

    assert_eq!(release_in(&checkout.dir), "two");
    assert!(!checkout.dir.join("left-over").exists());
}

/// A folder that is not a repository is not judged by the one around it.
#[test]
fn a_folder_that_is_not_a_repository_is_replaced() {
    let tmp = tempfile::tempdir().unwrap();
    let url = upstream(tmp.path());
    // The checkout's parent is a repository of its own, with no commits.
    let outer = tmp.path().join("outer");
    std::fs::create_dir_all(&outer).unwrap();
    git(&outer, &["init", "-q"]);
    let home = outer.join("src");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::write(home.join("stray"), "x").unwrap();

    let checkout = prepare(&url, &tag("b1"), &home).0.expect("clones b1");

    assert_eq!(checkout.dir, staging(&home));
    checkout.settle().expect("moves into place");
    assert_eq!(release_in(&home), "one");
    assert!(!home.join("stray").exists());
}

#[test]
fn following_upstream_never_reuses_and_records_the_commit() {
    let tmp = tempfile::tempdir().unwrap();
    let url = upstream(tmp.path());
    let home = tmp.path().join("src");
    installed(&url, "b2", &home);

    let checkout = prepare(&url, &ReleaseSelector::Latest, &home)
        .0
        .expect("clones");

    assert_eq!(checkout.dir, staging(&home));
    assert_eq!(checkout.tag, None);
    assert_eq!(
        checkout.commit_sha,
        git(&upstream_dir(tmp.path()), &["rev-parse", "main"])
    );
    assert_eq!(checkout.version, checkout.short_sha);
}

#[test]
fn a_download_retires_the_checkout_and_what_a_build_left_beside_it() {
    let tmp = tempfile::tempdir().unwrap();
    let url = upstream(tmp.path());
    let home = tmp.path().join("src");
    installed(&url, "b1", &home);
    for left in leftovers(&home) {
        std::fs::create_dir_all(&left).unwrap();
    }

    assert!(retire_checkout(&home).expect("retires"));

    assert!(!home.exists());
    for left in leftovers(&home) {
        assert!(!left.exists(), "{} is left", left.display());
    }
    assert!(
        !retire_checkout(&home).expect("nothing there"),
        "nothing left to remove"
    );
}

#[test]
fn a_download_keeps_a_checkout_with_work_of_its_own() {
    let tmp = tempfile::tempdir().unwrap();
    let url = upstream(tmp.path());
    let home = tmp.path().join("src");
    installed(&url, "b1", &home);
    std::fs::write(home.join("release.txt"), "patched").unwrap();

    assert!(!retire_checkout(&home).expect("keeps it"));

    assert_eq!(release_in(&home), "patched");
}
