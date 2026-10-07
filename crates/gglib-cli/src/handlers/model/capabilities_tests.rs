//! `gglib model capabilities --set` and `--unset`, by the names the command
//! prints: each name moves its own bit of the stored model, and a name that
//! is no flag is refused before the command runs.

use gglib_core::ModelCapabilities;

use crate::handlers::model::test_library::{library, run, stored};

/// Each flag as the command names it, beside the bit it is.
const NAMED: [(&str, ModelCapabilities); 4] = [
    (
        "supports-system-role",
        ModelCapabilities::SUPPORTS_SYSTEM_ROLE,
    ),
    (
        "requires-strict-turns",
        ModelCapabilities::REQUIRES_STRICT_TURNS,
    ),
    (
        "supports-tool-calls",
        ModelCapabilities::SUPPORTS_TOOL_CALLS,
    ),
    ("supports-reasoning", ModelCapabilities::SUPPORTS_REASONING),
];

#[tokio::test]
async fn each_flag_name_sets_and_clears_its_own_bit() {
    let dir = tempfile::tempdir().unwrap();
    let (ctx, model) = library(dir.path()).await;
    let id = model.id.to_string();
    let command = ["gglib", "model", "capabilities", &id];
    let before = model.capabilities;

    for (name, bit) in NAMED {
        run(&ctx, &[&command[..], &["--set", name]].concat())
            .await
            .expect("the flag is set");
        assert_eq!(stored(&ctx, model.id).await.capabilities, before | bit);

        run(&ctx, &[&command[..], &["--unset", name]].concat())
            .await
            .expect("the flag is cleared");
        assert_eq!(stored(&ctx, model.id).await.capabilities, before - bit);
    }
}

#[tokio::test]
async fn a_name_that_is_no_flag_is_refused_with_the_names_that_are() {
    let dir = tempfile::tempdir().unwrap();
    let (ctx, model) = library(dir.path()).await;
    let id = model.id.to_string();

    for option in ["--set", "--unset"] {
        let argv = ["gglib", "model", "capabilities", &id, option, "nope"];
        let refused = run(&ctx, &argv).await.expect_err("no such flag");

        let said = refused.to_string();
        assert!(said.contains("invalid value 'nope'"), "{said}");
        for (name, _) in NAMED {
            assert!(said.contains(name), "{name} is not offered in: {said}");
        }
    }
    assert_eq!(
        stored(&ctx, model.id).await.capabilities,
        model.capabilities
    );
}
