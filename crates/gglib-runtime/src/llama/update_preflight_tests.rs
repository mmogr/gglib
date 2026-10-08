//! The update preflight's rule, state by state.
//!
//! The command and the route both show a refusal's own text, so the verdict
//! and the advice asserted here are the ones each of them gives.
//!
//! How the facts are gathered is not tested here: that takes a machine whose
//! git and build tools a test can set, which `gglib-cli`'s
//! `llama_update_preflight` suite makes out of stand-ins.

use super::*;

/// An install an update can start on: a binary, its checkout, the tools.
fn updatable() -> UpdateFacts {
    UpdateFacts {
        binary_installed: true,
        checkout_present: true,
        missing_tools: Vec::new(),
        local_changes: false,
    }
}

#[test]
fn nothing_installed_is_refused_and_told_to_install() {
    let facts = UpdateFacts::default();

    let refusal = judge(&facts).unwrap_err();

    assert_eq!(refusal, UpdateRefusal::NotInstalled);
    assert_eq!(
        refusal.to_string(),
        "llama.cpp is not installed.\nRun 'gglib config llama install' to install it."
    );
}

/// Not `install`, which says the binary is already there and stops, and with
/// `--force` downloads it again and still leaves no checkout.
#[test]
fn a_pre_built_install_is_refused_and_told_to_rebuild() {
    let facts = UpdateFacts {
        checkout_present: false,
        ..updatable()
    };

    let refusal = judge(&facts).unwrap_err();

    assert_eq!(refusal, UpdateRefusal::NoSourceCheckout);
    assert_eq!(
        refusal.to_string(),
        "There is no llama.cpp source checkout to update, as after a pre-built download.\n\
         Run 'gglib config llama rebuild' to build llama.cpp from source; an update works after that."
    );
}

#[test]
fn a_source_build_with_its_tools_may_update_and_is_told_nothing() {
    assert_eq!(judge(&updatable()), Ok(None));
}

#[test]
fn a_source_build_without_its_tools_is_refused_and_told_how_to_install_them() {
    let facts = UpdateFacts {
        missing_tools: vec!["cmake", "C++ compiler"],
        ..updatable()
    };

    let refusal = judge(&facts).unwrap_err();

    assert_eq!(
        refusal,
        UpdateRefusal::MissingTools(vec!["cmake", "C++ compiler"])
    );
    let install = build_tool_install_lines();
    assert_eq!(
        refusal.to_string(),
        format!(
            "An update rebuilds llama.cpp from source, and these build tools were not found: \
             cmake, C++ compiler.\n{}\nInstall them, then update again.",
            install.join("\n")
        )
    );
    assert!(install.len() >= 2, "a heading and a command: {install:?}");
}

/// A changed checkout was never refused, and git pulls over a change that
/// upstream did not touch, so the update starts and the user is told.
#[test]
fn a_checkout_with_local_changes_may_update_and_is_cautioned() {
    let facts = UpdateFacts {
        local_changes: true,
        ..updatable()
    };

    assert_eq!(judge(&facts), Ok(Some(LOCAL_CHANGES_CAUTION)));
    assert!(LOCAL_CHANGES_CAUTION.contains("local changes"));
}

/// The first thing wrong is the one reported: nothing is said about tools or
/// changes to an install that has no binary, or no checkout.
#[test]
fn the_refusals_come_in_the_order_they_would_be_put_right() {
    let mut facts = UpdateFacts {
        binary_installed: false,
        checkout_present: false,
        missing_tools: vec!["git"],
        local_changes: true,
    };
    assert_eq!(judge(&facts), Err(UpdateRefusal::NotInstalled));

    facts.binary_installed = true;
    assert_eq!(judge(&facts), Err(UpdateRefusal::NoSourceCheckout));

    facts.checkout_present = true;
    assert_eq!(judge(&facts), Err(UpdateRefusal::MissingTools(vec!["git"])));
}

/// The recorded acceleration wins, so an update never changes backend.
#[test]
fn an_update_rebuilds_with_the_acceleration_the_build_recorded() {
    for (recorded, acceleration) in [
        ("Metal", Acceleration::Metal),
        ("CUDA", Acceleration::Cuda),
        ("Vulkan", Acceleration::Vulkan),
    ] {
        let config = BuildConfig {
            version: "abc1234".into(),
            commit_sha: "abc1234def5678".into(),
            build_date: chrono::Utc::now(),
            acceleration: recorded.into(),
            cmake_flags: Vec::new(),
        };
        assert_eq!(acceleration_for(Some(&config)).unwrap(), acceleration);
    }
}
