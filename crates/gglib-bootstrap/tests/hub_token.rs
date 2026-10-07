//! The Hub token: the shared bootstrap reads it from the environment itself.

mod common;

use std::process::Command;

use tempfile::TempDir;

use common::build_core;

/// Not a token of any account.
const FAKE_TOKEN: &str = "hf_fake_token_for_a_test";

/// Set in the environment of the process this test starts, to tell it that
/// it is that process.
const STARTED_WITH_THE_TOKEN: &str = "GGLIB_TEST_STARTED_WITH_THE_HUB_TOKEN";

const THIS_TEST: &str = "a_core_built_here_holds_the_environments_hub_token";

/// No adapter hands the bootstrap a token, so none can leave it out: a core
/// built here holds the one in its process's environment, for the surfaces
/// that ask the Hub through it.
///
/// A process cannot safely change its own environment once it has threads,
/// so the test runs itself again in a process started with the token, and
/// builds the core there.
#[test]
fn a_core_built_here_holds_the_environments_hub_token() {
    if std::env::var_os(STARTED_WITH_THE_TOKEN).is_none() {
        let ran = Command::new(std::env::current_exe().expect("this test binary"))
            .args(["--exact", THIS_TEST])
            .env(STARTED_WITH_THE_TOKEN, "1")
            // Spelled out, as whoever sets it spells it.
            .env("HF_TOKEN", FAKE_TOKEN)
            .output()
            .expect("the test binary runs");
        let said = String::from_utf8_lossy(&ran.stdout);
        assert!(ran.status.success(), "{said}");
        assert!(said.contains("1 passed"), "the test did not run: {said}");
        return;
    }

    let dir = TempDir::new().unwrap();
    let core = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("a runtime")
        .block_on(build_core(&dir));

    assert_eq!(core.app.hf_token().as_deref(), Some(FAKE_TOKEN));
}
