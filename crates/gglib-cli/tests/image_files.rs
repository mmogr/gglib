//! `gglib q --image <path>` and `gglib chat --image <path>` up to the point
//! a model is looked up.
//!
//! The file is read before a model is looked up, so in a data directory
//! with no model, no default and no daemon a file that cannot be attached
//! fails the command on the file, by its path, and never reaches the "no
//! model" refusal that would otherwise come first; one that can be attached
//! gets its receipt line and then that refusal. These bind nothing and
//! contact nothing.

use std::path::Path;
use std::process::{Command, Output};

/// Run `gglib <args>` over the empty data directory `dir`, with nothing on
/// stdin.
fn gglib(dir: &tempfile::TempDir, args: &[&str], path: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_gglib"))
        .args(args)
        .arg(path)
        .arg("x")
        .env("GGLIB_DATA_DIR", dir.path())
        .stdin(std::process::Stdio::null())
        .output()
        .expect("running `gglib`")
}

/// Run `gglib q --image <path> x` over an empty data directory.
fn ask_with(dir: &tempfile::TempDir, path: &Path) -> Output {
    gglib(dir, &["q", "--image"], path)
}

/// A PNG's signature and its `IHDR` chunk, 64 by 32, in a file called
/// `shot.png` under `dir`.
fn shot(dir: &tempfile::TempDir) -> std::path::PathBuf {
    let mut bytes = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    bytes.extend([0, 0, 0, 13]);
    bytes.extend(b"IHDR");
    bytes.extend(64_u32.to_be_bytes());
    bytes.extend(32_u32.to_be_bytes());
    bytes.extend([8, 6, 0, 0, 0, 0, 0, 0, 0]);
    let path = dir.path().join("shot.png");
    std::fs::write(&path, bytes).expect("the file is written");
    path
}

/// The receipt line of [`shot`].
const RECEIPT: &str = "  image shot.png: 64x32, ~2 tokens";

#[test]
fn a_missing_image_fails_by_its_path_before_a_model_is_looked_up() {
    let dir = tempfile::tempdir().expect("temp data dir");
    let path = dir.path().join("nonexistent.png");

    let out = ask_with(&dir, &path);

    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(1), "{stderr}");
    assert!(
        stderr.contains(&format!("cannot read image '{}'", path.display())),
        "{stderr}"
    );
    assert!(!stderr.contains("No model specified"), "{stderr}");
    assert!(out.stdout.is_empty());
}

#[test]
fn a_text_file_fails_by_its_path_and_names_what_is_read() {
    let dir = tempfile::tempdir().expect("temp data dir");
    let path = dir.path().join("notes.txt");
    std::fs::write(&path, "plain text").expect("the file is written");

    let out = ask_with(&dir, &path);

    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(1), "{stderr}");
    assert!(
        stderr.contains(&format!("image '{}'", path.display())),
        "{stderr}"
    );
    assert!(stderr.contains("only PNG and JPEG are read"), "{stderr}");
    assert!(!stderr.contains("No model specified"), "{stderr}");
    assert!(out.stdout.is_empty());
}

/// `gglib chat --image <path> x`: the flag reaches the chat, which fails on
/// the file before it asks which model `x` is.
#[test]
fn chat_with_a_missing_image_fails_by_its_path_before_a_model_is_looked_up() {
    let dir = tempfile::tempdir().expect("temp data dir");
    let path = dir.path().join("nonexistent.png");

    let out = gglib(&dir, &["chat", "--image"], &path);

    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(1), "{stderr}");
    assert!(
        stderr.contains(&format!("cannot read image '{}'", path.display())),
        "{stderr}"
    );
}

/// `gglib q` prints the receipt of an image it attached, and `-Q` keeps it
/// quiet. Both go on to the refusal of a question with no model.
#[test]
fn q_prints_an_images_receipt_and_quiet_suppresses_it() {
    let dir = tempfile::tempdir().expect("temp data dir");
    let path = shot(&dir);

    let loud = gglib(&dir, &["q", "--image"], &path);
    let quiet = gglib(&dir, &["q", "-Q", "--image"], &path);

    let loud = String::from_utf8_lossy(&loud.stderr);
    assert!(loud.lines().any(|line| line == RECEIPT), "{loud}");
    assert!(loud.contains("No model specified"), "{loud}");
    let quiet = String::from_utf8_lossy(&quiet.stderr);
    assert!(!quiet.contains("image shot.png"), "{quiet}");
    assert!(quiet.contains("No model specified"), "{quiet}");
}

/// `gglib chat` has no quiet flag: it always prints the receipt.
#[test]
fn chat_prints_an_images_receipt() {
    let dir = tempfile::tempdir().expect("temp data dir");
    let path = shot(&dir);

    let out = gglib(&dir, &["chat", "--image"], &path);

    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.lines().any(|line| line == RECEIPT), "{stderr}");
    // It ended where the model `x` was looked up, and went no further.
    assert!(stderr.contains("No model found matching: 'x'"), "{stderr}");
    assert_eq!(out.status.code(), Some(1), "{stderr}");
}
