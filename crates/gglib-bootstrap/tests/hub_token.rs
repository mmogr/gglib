//! The Hub token: the shared bootstrap wires the one its caller hands it, and
//! reads the environment's itself when it is handed none.

mod common;

use std::process::Command;

use tempfile::TempDir;

use gglib_bootstrap::{BootstrapConfig, BuiltCore, CoreBootstrap};

use common::{minimal_config, noop_emitter};

/// The token in the environment of the process a test starts. Not a token of
/// any account.
const ENV_TOKEN: &str = "hf_fake_token_for_a_test";

/// A token a caller hands the bootstrap. Not a token of any account.
const HANDED_TOKEN: &str = "hf_fake_token_handed_to_the_bootstrap";

/// Set in the environment of the process a test starts, to tell it that it
/// is that process.
const STARTED_WITH_THE_TOKEN: &str = "GGLIB_TEST_STARTED_WITH_THE_HUB_TOKEN";

/// The daemon and the CLI hand the bootstrap no token, so a core built for
/// them holds the one in their process's environment, for the surfaces that
/// ask the Hub through it.
#[test]
fn a_core_built_here_holds_the_environments_hub_token() {
    if !started_with_the_token("a_core_built_here_holds_the_environments_hub_token") {
        return;
    }
    let dir = TempDir::new().unwrap();

    let core = built(minimal_config(&dir));

    assert_eq!(core.app.hf_token().as_deref(), Some(ENV_TOKEN));
}

/// A caller that keeps its own token, as an app on a phone does, gets that
/// one wired, whatever the environment holds.
#[test]
fn a_token_handed_to_the_bootstrap_wins_over_the_environments() {
    if !started_with_the_token("a_token_handed_to_the_bootstrap_wins_over_the_environments") {
        return;
    }
    let dir = TempDir::new().unwrap();
    let config = BootstrapConfig {
        hf_token: Some(HANDED_TOKEN.to_owned()),
        ..minimal_config(&dir)
    };

    let core = built(config);

    assert_eq!(core.app.hf_token().as_deref(), Some(HANDED_TOKEN));
}

/// Whether this is the process `test` runs in with `HF_TOKEN` set. When it
/// is not, `test` is run in such a process, and must pass there.
///
/// A process cannot safely change its own environment once it has threads,
/// so a test of what the bootstrap reads from it runs itself again in a
/// process started with the token, and builds the core there.
fn started_with_the_token(test: &str) -> bool {
    if std::env::var_os(STARTED_WITH_THE_TOKEN).is_some() {
        return true;
    }
    let ran = Command::new(std::env::current_exe().expect("this test binary"))
        .args(["--exact", test])
        .env(STARTED_WITH_THE_TOKEN, "1")
        // Spelled out, as whoever sets it spells it.
        .env("HF_TOKEN", ENV_TOKEN)
        .output()
        .expect("the test binary runs");
    let said = String::from_utf8_lossy(&ran.stdout);
    assert!(ran.status.success(), "{said}");
    assert!(said.contains("1 passed"), "the test did not run: {said}");
    false
}

/// The core `config` builds, on a runtime of its own.
fn built(config: BootstrapConfig) -> BuiltCore {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("a runtime")
        .block_on(CoreBootstrap::build(config, noop_emitter()))
        .expect("CoreBootstrap::build must succeed with valid config")
}
