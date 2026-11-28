//! What `gglib model add`, `update`, `retag` and `remove` print, run as a
//! person runs them against a library on disk.
//!
//! Each command changes the library through `ModelOps`. What they say when
//! they have is the handlers' own, and scripts read it.

#[path = "support/library.rs"]
mod library;

#[path = "support/data_dir.rs"]
mod data_dir;

use std::path::Path;

use gglib_core::Model;
use gglib_core::domain::{ReasoningEffort, TemplateCaps};
use library::{gglib, library, run, write_gguf};

/// The rule under the preview's heading.
const RULE: &str = "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━";

#[test]
fn an_answered_update_prints_its_preview_and_that_it_updated() {
    let root = tempfile::tempdir().expect("temp data dir");
    library(root.path());

    let set = gglib(
        root.path(),
        &[
            "model",
            "update",
            "1",
            "--name",
            "Renamed",
            "--temperature",
            "0.7",
            "--yes",
        ],
        "",
    );
    let cleared = gglib(
        root.path(),
        &[
            "model",
            "update",
            "Renamed",
            "--unset",
            "temperature",
            "--yes",
        ],
        "",
    );

    assert_eq!(
        set,
        format!(
            "\n📋 Preview of changes for model ID 1:\n{RULE}\n\
             \x20 Name:           qwen.Q8_0 → Renamed\n\
             \x20 Inference Defaults:\n\
             \x20   + Set model-specific defaults:\n\
             \x20     Temperature: 0.7\n\
             ✓ Model updated successfully!\n"
        )
    );
    assert_eq!(
        cleared,
        format!(
            "\n📋 Preview of changes for model ID 1:\n{RULE}\n\
             \x20 Inference Defaults:\n\
             \x20   ✗ Cleared (will inherit from global/hardcoded)\n\
             ✓ Model updated successfully!\n"
        )
    );
}

#[test]
fn an_answered_remove_prints_the_one_line() {
    let root = tempfile::tempdir().expect("temp data dir");
    library(root.path());

    let removed = gglib(root.path(), &["model", "remove", "1", "--yes"], "");

    assert_eq!(
        removed,
        "✓ Model 'qwen.Q8_0' (ID 1) successfully removed from database.\n"
    );
}

/// What an add prints of the file before it asks anything.
const READ_FROM_THE_FILE: &str = "File validation and metadata extraction successful.\n\
     \nExtracted metadata:\n\
     \x20 Name: qwen.Q8_0\n\
     \x20 Architecture: qwen3\n\
     \x20 Quantization: Q8_0\n";

/// The file carries no parameter count, so an add is given one, and stores
/// and prints it. A re-import asks nothing and leaves it stored.
#[test]
fn an_add_stores_the_count_given_and_a_reimport_leaves_it() {
    let root = tempfile::tempdir().expect("temp data dir");
    let weights = write_gguf(root.path(), "qwen.Q8_0.gguf");
    let file = weights.to_str().expect("a path in UTF-8");
    let stored = format!(
        "\x20 Name: qwen.Q8_0\n\
         \x20 File: {file}\n\
         \x20 Parameters: 7.5B\n\
         \x20 Architecture: qwen3\n\
         \x20 Quantization: Q8_0\n"
    );

    let added = gglib(
        root.path(),
        &["model", "add", file, "--param-count", "7.5"],
        "",
    );
    let reimported = gglib(root.path(), &["model", "add", file, "--reimport"], "");

    assert_eq!(
        added,
        format!(
            "{READ_FROM_THE_FILE}\
             \nModel successfully created:\n\n\
             {stored}\
             Model successfully added to database!\n"
        )
    );
    assert_eq!(
        reimported,
        format!(
            "{READ_FROM_THE_FILE}\
             \nSkipping the parameter-count prompt: --reimport refreshes derived metadata \
             only and leaves the stored parameter count alone.\n\
             \nRe-derived from file:\n\n\
             {stored}\
             Replaced: tags, capabilities.\n\
             Updated where newly detected: quantization, context length, expert counts.\n\
             Unchanged: name, parameter count, architecture.\n"
        )
    );
}

/// A file that is already in the library is refused in the handler's words,
/// before anything is asked or refused for want of a terminal.
#[test]
fn an_add_of_a_file_already_in_the_library_is_refused_before_it_asks() {
    let root = tempfile::tempdir().expect("temp data dir");
    let weights = library(root.path());
    let file = weights.to_str().expect("a path in UTF-8");

    let refused = run(root.path(), &["model", "add", file], "");

    assert_eq!(refused.status.code(), Some(1));
    assert_eq!(
        String::from_utf8_lossy(&refused.stdout),
        "File validation and metadata extraction successful.\n"
    );
    assert_eq!(
        String::from_utf8_lossy(&refused.stderr),
        format!(
            "Error: '{file}' is already in the library as \"qwen.Q8_0\" (id 1).\n\
             Pass --reimport to re-import it and refresh its derived metadata.\n"
        )
    );
}

/// A retag says of each model what the pass added, removed and re-derived,
/// or that it was up to date, and then how many models it changed.
///
/// The model's stored chat template is what detection reads. One that
/// thinks and calls tools derives two tags; an additive
/// pass adds them and, once the template is gone, keeps them, and a full
/// pass drops them.
#[test]
fn a_retag_prints_what_each_pass_changed_and_how_many_models_it_updated() {
    let root = tempfile::tempdir().expect("temp data dir");
    library(root.path());
    let edit = |change: [&str; 2]| {
        let argv = [&["model", "update", "1"][..], &change, &["--yes"]].concat();
        gglib(root.path(), &argv, "");
    };

    let unchanged = gglib(root.path(), &["model", "retag", "--all"], "");
    edit(["--metadata", "tokenizer.chat_template=<think><tool_call>"]);
    let added = gglib(root.path(), &["model", "retag", "1"], "");
    edit(["--remove-metadata", "tokenizer.chat_template"]);
    let kept = gglib(root.path(), &["model", "retag", "qwen.Q8_0"], "");
    let removed = gglib(root.path(), &["model", "retag", "--all", "--full"], "");

    assert_eq!(
        unchanged,
        "Retagging 1 model(s) (additive) ...\n\
         \x20 [1] qwen.Q8_0 — already up to date\n\
         Done. 0 model(s) updated.\n"
    );
    assert_eq!(
        added,
        "Retagging 1 model(s) (additive) ...\n\
         \x20 [1] qwen.Q8_0 — added: agent, reasoning\n\
         Done. 1 model(s) updated.\n"
    );
    assert_eq!(kept, unchanged);
    assert_eq!(
        removed,
        "Retagging 1 model(s) (full rebuild) ...\n\
         \x20 [1] qwen.Q8_0 — removed: agent, reasoning\n\
         Done. 1 model(s) updated.\n"
    );
}

// ── An effort level the template does not list ────────────────────────────────

/// Model 1's row, read straight from the library in `root`.
fn model_one(root: &Path) -> Model {
    data_dir::read_model(&library::data_root(root), 1)
}

/// Model 1 with caps recorded whose template lists `levels`, as a served
/// model's are.
fn listing(root: &Path, levels: &[&str]) {
    data_dir::update_model(&library::data_root(root), 1, |model| {
        model.template_caps = Some(TemplateCaps {
            supports_reasoning_effort: Some(true),
            reasoning_effort_levels: Some(levels.iter().map(|&l| l.to_owned()).collect()),
            ..TemplateCaps::default()
        });
    });
}

/// What `gglib model update 1 --reasoning-effort max` says on Qwen3.8's list.
fn qwen_note(done: &str) -> String {
    format!(
        "Note: this model's template lists the reasoning-effort levels low, medium, xhigh, \
         and not max. The level {done} all the same: one the list leaves out can still work.\n"
    )
}

#[test]
fn an_update_to_a_level_the_template_does_not_list_notes_it_and_stores_it() {
    let root = tempfile::tempdir().expect("temp data dir");
    library(root.path());
    listing(root.path(), &["low", "medium", "xhigh"]);

    let updated = run(
        root.path(),
        &["model", "update", "1", "--reasoning-effort", "max", "--yes"],
        "",
    );

    assert!(updated.status.success(), "{updated:?}");
    assert_eq!(
        String::from_utf8_lossy(&updated.stderr),
        qwen_note("is stored")
    );
    assert!(
        String::from_utf8_lossy(&updated.stdout).ends_with("✓ Model updated successfully!\n"),
        "{updated:?}"
    );
    let stored = model_one(root.path()).inference_defaults;
    assert_eq!(
        stored.and_then(|d| d.reasoning_effort),
        Some(ReasoningEffort::Max)
    );
}

#[test]
fn a_dry_run_notes_the_level_and_stores_nothing() {
    let root = tempfile::tempdir().expect("temp data dir");
    library(root.path());
    listing(root.path(), &["low", "medium", "xhigh"]);

    let previewed = run(
        root.path(),
        &[
            "model",
            "update",
            "1",
            "--reasoning-effort",
            "max",
            "--dry-run",
        ],
        "",
    );

    assert!(previewed.status.success(), "{previewed:?}");
    assert_eq!(
        String::from_utf8_lossy(&previewed.stderr),
        qwen_note("would be stored")
    );
    assert_eq!(model_one(root.path()).inference_defaults, None);
}

/// Without `--yes` and with no terminal the confirm refuses, so the note
/// says the level would be stored, and nothing is.
#[test]
fn an_unconfirmed_update_notes_the_level_as_one_that_would_be_stored() {
    let root = tempfile::tempdir().expect("temp data dir");
    library(root.path());
    listing(root.path(), &["low", "medium", "xhigh"]);

    let refused = run(
        root.path(),
        &["model", "update", "1", "--reasoning-effort", "max"],
        "",
    );

    assert!(!refused.status.success(), "{refused:?}");
    let stderr = String::from_utf8_lossy(&refused.stderr);
    assert!(
        stderr.starts_with(&qwen_note("would be stored")),
        "{stderr}"
    );
    assert!(stderr.contains("the model was not updated"), "{stderr}");
    assert_eq!(model_one(root.path()).inference_defaults, None);
}

/// A listed level, any level while the list is unknown, and a level cleared
/// in the same command earn no note.
#[test]
fn a_listed_unknown_or_cleared_level_earns_no_note() {
    let unknown = tempfile::tempdir().expect("temp data dir");
    library(unknown.path());
    let listed = tempfile::tempdir().expect("temp data dir");
    library(listed.path());
    listing(listed.path(), &["low", "medium", "xhigh"]);

    let cases: [(&tempfile::TempDir, &[&str]); 3] = [
        (&unknown, &["--reasoning-effort", "max"]),
        (&listed, &["--reasoning-effort", "xhigh"]),
        (
            &listed,
            &["--reasoning-effort", "max", "--unset", "reasoning-effort"],
        ),
    ];
    for (root, flags) in cases {
        let argv = [&["model", "update", "1", "--yes"][..], flags].concat();
        let updated = run(root.path(), &argv, "");
        assert!(updated.status.success(), "{updated:?}");
        let stderr = String::from_utf8_lossy(&updated.stderr);
        assert!(!stderr.contains("Note:"), "for {flags:?}: {stderr}");
    }
    let cleared = model_one(listed.path()).inference_defaults;
    assert_eq!(cleared.and_then(|d| d.reasoning_effort), None);
}
