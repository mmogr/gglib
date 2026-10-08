//! What `gglib model add`, `update`, `retag` and `remove` print, run as a
//! person runs them against a library on disk.
//!
//! Each command changes the library through `ModelOps`. What they say when
//! they have is the handlers' own, and scripts read it.

#[path = "support/library.rs"]
mod library;

use library::{gglib, library, run, write_gguf};

/// The rule under the preview's heading.
const RULE: &str = "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━";

#[test]
fn a_forced_update_prints_its_preview_and_that_it_updated() {
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
            "--force",
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
            "--force",
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
fn a_forced_remove_prints_the_one_line() {
    let root = tempfile::tempdir().expect("temp data dir");
    library(root.path());

    let removed = gglib(root.path(), &["model", "remove", "1", "--force"], "");

    assert_eq!(
        removed,
        "✓ Model 'qwen.Q8_0' (ID 1) successfully removed from database.\n"
    );
}

/// Asked first, a removal also says the file is still there.
#[test]
fn a_confirmed_remove_ends_by_saying_the_file_remains() {
    let root = tempfile::tempdir().expect("temp data dir");
    let weights = library(root.path());

    let removed = gglib(root.path(), &["model", "remove", "qwen.Q8_0"], "y\n");

    let ending = format!(
        "✓ Model 'qwen.Q8_0' (ID 1) successfully removed from database.\n\
         Note: The model file '{}' remains on disk.\n",
        weights.display()
    );
    assert!(removed.ends_with(&ending), "{removed}");
    assert!(weights.exists(), "the file went with the row");
}

/// What an add prints of the file before it asks anything.
const READ_FROM_THE_FILE: &str = "File validation and metadata extraction successful.\n\
     \nExtracted metadata:\n\
     \x20 Name: qwen.Q8_0\n\
     \x20 Architecture: qwen3\n\
     \x20 Quantization: Q8_0\n";

/// The file carries no parameter count, so an add asks for one, and stores
/// and prints the one typed. A re-import asks nothing and leaves it stored.
#[test]
fn an_add_stores_the_count_typed_and_a_reimport_leaves_it() {
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

    let added = gglib(root.path(), &["model", "add", file], "7.5\n");
    let reimported = gglib(root.path(), &["model", "add", file, "--reimport"], "");

    assert_eq!(
        added,
        format!(
            "{READ_FROM_THE_FILE}\
             Parameter count (in billions): \n\
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
             Replaced: tags, capabilities, dialect spec.\n\
             Updated where newly detected: quantization, context length, expert counts.\n\
             Unchanged: name, parameter count, architecture.\n"
        )
    );
}

/// A file that is already in the library is refused in the handler's words,
/// before anything is asked: the count typed here is never read.
#[test]
fn an_add_of_a_file_already_in_the_library_is_refused_before_it_asks() {
    let root = tempfile::tempdir().expect("temp data dir");
    let weights = library(root.path());
    let file = weights.to_str().expect("a path in UTF-8");

    let refused = run(root.path(), &["model", "add", file], "9\n");

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
/// thinks and calls tools derives three tags and a dialect spec; an additive
/// pass adds them and, once the template is gone, keeps them, and a full
/// pass drops them.
#[test]
fn a_retag_prints_what_each_pass_changed_and_how_many_models_it_updated() {
    let root = tempfile::tempdir().expect("temp data dir");
    library(root.path());
    let edit = |change: [&str; 2]| {
        let argv = [&["model", "update", "1"][..], &change, &["--force"]].concat();
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
         \x20 [1] qwen.Q8_0 — added: agent, format:hermes, reasoning\n\
         \x20 [1] qwen.Q8_0 — dialect spec re-derived\n\
         Done. 1 model(s) updated.\n"
    );
    assert_eq!(kept, unchanged);
    assert_eq!(
        removed,
        "Retagging 1 model(s) (full rebuild) ...\n\
         \x20 [1] qwen.Q8_0 — removed: agent, format:hermes, reasoning\n\
         \x20 [1] qwen.Q8_0 — dialect spec re-derived\n\
         Done. 1 model(s) updated.\n"
    );
}
