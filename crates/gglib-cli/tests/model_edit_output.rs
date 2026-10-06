//! What `gglib model update` and `gglib model remove` print when they
//! succeed, run as a person runs them against a library on disk.
//!
//! Both commands change the library through `ModelOps`. What they say when
//! they have is the handlers' own, and scripts read it.

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// The rule under the preview's heading.
const RULE: &str = "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━";

/// A GGUF v3 file in `dir` whose only metadata is its architecture, under
/// its canonical path.
fn write_gguf(dir: &Path, name: &str) -> PathBuf {
    let string = |text: &str| {
        let mut bytes = (text.len() as u64).to_le_bytes().to_vec();
        bytes.extend_from_slice(text.as_bytes());
        bytes
    };
    let mut bytes = b"GGUF".to_vec();
    bytes.extend_from_slice(&3_u32.to_le_bytes());
    bytes.extend_from_slice(&0_u64.to_le_bytes());
    bytes.extend_from_slice(&1_u64.to_le_bytes());
    bytes.extend(string("general.architecture"));
    bytes.extend_from_slice(&8_u32.to_le_bytes());
    bytes.extend(string("qwen3"));
    let path = dir.join(name);
    std::fs::write(&path, bytes).expect("the file is written");
    path.canonicalize().expect("its canonical path")
}

/// `gglib <args>` over the library in `root`, answered `typed` at its
/// prompts: what it printed, once it has succeeded.
fn gglib(root: &Path, args: &[&str], typed: &str) -> String {
    let mut child = Command::new(env!("CARGO_BIN_EXE_gglib"))
        .args(args)
        .env("GGLIB_DATA_DIR", root.join("data"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap_or_else(|e| panic!("running `gglib {}`: {e}", args.join(" ")));
    child
        .stdin
        .take()
        .expect("its stdin")
        .write_all(typed.as_bytes())
        .expect("the answers are typed");
    let out = child.wait_with_output().expect("the command ends");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    assert!(
        out.status.success(),
        "`gglib {}` must succeed\nstdout: {stdout}\nstderr: {}",
        args.join(" "),
        String::from_utf8_lossy(&out.stderr)
    );
    stdout
}

/// A library in `root` holding one model, id 1, named for its file. The
/// file carries no parameter count, so `model add` asks for one.
fn library(root: &Path) -> PathBuf {
    let weights = write_gguf(root, "qwen.Q8_0.gguf");
    gglib(root, &["model", "add", weights.to_str().unwrap()], "7\n");
    weights
}

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
