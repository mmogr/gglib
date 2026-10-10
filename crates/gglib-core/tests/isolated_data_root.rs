//! With a test binary's own data root set, every path built on the data and
//! resource roots resolves under it, and not into the checkout a debug build
//! otherwise answers (#955).
//!
//! A binary of its own because the root is set once per process and comes
//! before `GGLIB_DATA_DIR`: this crate's unit tests point that variable at
//! roots of their own, which a root set in their process would override.
//! Built only with the `test-utils` feature (see `Cargo.toml`).

use std::path::Path;
use std::process::Command;

use gglib_core::paths::{
    data_root, database_path, isolate_data_root, llama_server_path, pids_dir, remote_identity_path,
    resource_root, sd_config_path, sd_cpp_dir, sd_server_path,
};

/// The child's name in this binary. A rename leaves it unrun, and the parent
/// then fails on the missing marker rather than passing.
const CHILD: &str = "the_child_resolves_with_both_variables_set";

/// Set by the parent, so that an ordinary run leaves the child a no-op.
const CHILD_ENV: &str = "GGLIB_TEST_ROOT_CHILD";

/// Printed by the child once both roots resolved to its own.
const MARKER: &str = "TEST-ROOT-CAME-BEFORE-THE-ENVIRONMENT";

#[test]
fn every_resolver_answers_under_the_test_root() {
    let root = isolate_data_root();

    assert_eq!(data_root().unwrap(), root);
    assert_eq!(resource_root().unwrap(), root);
    assert_eq!(database_path().unwrap(), root.join("data").join("gglib.db"));
    assert_eq!(pids_dir().unwrap(), root.join("pids"));
    assert_eq!(
        remote_identity_path().unwrap(),
        root.join("data").join("remote_identity")
    );
    // Through `resource_root`, not `data_root`, so `GGLIB_DATA_DIR` alone would
    // not move it. It is the binary a pidfile sweep compares a process with.
    let llama = llama_server_path().unwrap();
    assert_eq!(
        llama.parent(),
        Some(root.join(".llama").join("bin").as_path())
    );
    // stable-diffusion.cpp's install, through the same root, under a
    // directory of its own that an uninstall removes whole.
    let sd = root.join(".sd");
    assert_eq!(
        sd_server_path().unwrap().parent(),
        Some(sd.join("bin").as_path())
    );
    assert_eq!(sd_config_path().unwrap(), sd.join("sd-config.json"));
    assert_eq!(sd_cpp_dir().unwrap(), sd.join("stable-diffusion.cpp"));
    // The daemon lock is `<data root>/daemon.lock`: `run_daemon` and
    // `gglib daemon status` both name the directory with `data_root()`.
}

#[test]
fn the_log_directory_is_under_the_test_root() {
    let root = isolate_data_root();

    gglib_core::telemetry::init_tracing(false).unwrap();

    assert!(
        root.join("logs").is_dir(),
        "init_tracing made its log directory somewhere else"
    );
}

#[test]
fn the_root_is_a_temporary_directory_set_once() {
    let root = isolate_data_root();
    let checkout = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");

    assert_eq!(isolate_data_root(), root);
    assert!(root.is_dir());
    assert!(root.starts_with(std::env::temp_dir()));
    assert!(!root.starts_with(checkout.canonicalize().unwrap()));
}

/// The test root comes before `GGLIB_DATA_DIR` and `GGLIB_RESOURCE_DIR`, so a
/// variable left in a developer's shell cannot point a test at a live root.
///
/// Setting a variable in this process needs `unsafe`, which the workspace
/// denies, so the child is this binary run again with both set.
#[test]
fn the_test_root_comes_before_the_environment() {
    let elsewhere = isolate_data_root().join("from-the-environment");

    let out = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", CHILD, "--nocapture", "--test-threads=1"])
        .env(CHILD_ENV, "1")
        .env("GGLIB_DATA_DIR", &elsewhere)
        .env("GGLIB_RESOURCE_DIR", &elsewhere)
        .output()
        .expect("run this binary again for the child");

    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "the child failed:\n{stdout}\n{stderr}"
    );
    assert!(stdout.contains(MARKER), "the child did not run:\n{stdout}");
}

#[test]
fn the_child_resolves_with_both_variables_set() {
    if std::env::var_os(CHILD_ENV).is_none() {
        return;
    }
    assert!(std::env::var_os("GGLIB_DATA_DIR").is_some());
    assert!(std::env::var_os("GGLIB_RESOURCE_DIR").is_some());
    let root = isolate_data_root();

    assert_eq!(data_root().unwrap(), root);
    assert_eq!(resource_root().unwrap(), root);
    println!("{MARKER}");
}
