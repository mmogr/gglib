//! A library on disk for a test to run the built `gglib` over, as a person
//! runs it: one model, in a data root under a temporary directory.
//!
//! Lives in a subdirectory because anything directly under `tests/` is built
//! as its own test binary; `#[path]`-included from the suites that need it.

#![allow(dead_code)]

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

/// The data root `gglib` runs under for the library in `root`.
pub(crate) fn data_root(root: &Path) -> PathBuf {
    root.join("data")
}

/// Where `gglib` looks for llama.cpp for the library in `root`. Nothing is
/// there unless the test puts it there.
pub(crate) fn resource_root(root: &Path) -> PathBuf {
    root.join("resources")
}

/// A GGUF v3 file in `dir` whose only metadata is its architecture, under
/// its canonical path.
pub(crate) fn write_gguf(dir: &Path, name: &str) -> PathBuf {
    let path = dir.join(name);
    gglib_gguf::write_string_gguf(&path, &[("general.architecture", "qwen3")]);
    path.canonicalize().expect("its canonical path")
}

/// `gglib <args>` over the library in `root`, answered `typed` at its
/// prompts: how it ended, and what it printed.
pub(crate) fn run(root: &Path, args: &[&str], typed: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_gglib"))
        .args(args)
        .env("GGLIB_DATA_DIR", data_root(root))
        .env("GGLIB_RESOURCE_DIR", resource_root(root))
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
    child.wait_with_output().expect("the command ends")
}

/// [`run`], once it has succeeded: what it printed.
pub(crate) fn gglib(root: &Path, args: &[&str], typed: &str) -> String {
    let out = run(root, args, typed);
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
pub(crate) fn library(root: &Path) -> PathBuf {
    let weights = write_gguf(root, "qwen.Q8_0.gguf");
    gglib(root, &["model", "add", weights.to_str().unwrap()], "7\n");
    weights
}
