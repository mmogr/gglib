//! What `config llama update` says it will do before it asks.

use super::*;
use gglib_runtime::llama::{Acceleration, BuildConfig, LOCAL_CHANGES_CAUTION};
use std::path::PathBuf;

fn plan(recorded: bool, caution: Option<&str>) -> UpdatePlan {
    UpdatePlan {
        acceleration: Acceleration::Metal,
        llama_dir: PathBuf::from("/data/.llama/llama.cpp"),
        server_path: PathBuf::from("/data/.llama/bin/llama-server"),
        recorded: recorded.then(|| BuildConfig {
            version: "abc1234".to_owned(),
            commit_sha: "abc1234def5678".to_owned(),
            build_date: chrono::Utc::now(),
            acceleration: "Metal".to_owned(),
            cmake_flags: vec!["-DGGML_METAL=ON".to_owned()],
        }),
        caution: caution.map(str::to_owned),
    }
}

#[test]
fn the_plan_names_the_build_being_replaced_and_what_will_happen() {
    assert_eq!(
        plan_lines(&plan(true, None)),
        [
            "Updating llama.cpp...",
            "",
            "Current version: abc1234",
            "Build config: Metal",
            "",
            "This will:",
            "  - Pull latest llama.cpp changes",
            "  - Rebuild with Metal support",
            "  - Replace current binary",
            "",
            "Current models will NOT be affected.",
            "",
        ]
    );
}

#[test]
fn a_build_that_left_no_record_is_not_named() {
    let lines = plan_lines(&plan(false, None));

    assert_eq!(lines[..4], ["Updating llama.cpp...", "", "", "This will:"]);
    assert!(!lines.iter().any(|line| line.starts_with("Current version")));
}

/// The caution is the last thing said before the question, where it is read.
#[test]
fn a_caution_is_said_last_before_the_question() {
    let lines = plan_lines(&plan(true, Some(LOCAL_CHANGES_CAUTION)));

    assert_eq!(
        lines[lines.len() - 4..],
        [
            "Current models will NOT be affected.",
            "",
            LOCAL_CHANGES_CAUTION,
            "",
        ]
    );
}
