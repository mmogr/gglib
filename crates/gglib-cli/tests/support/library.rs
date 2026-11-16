//! A library on disk for a test to run the built `gglib` over, as a person
//! runs it: one model, in a data root under a temporary directory.
//!
//! Lives in a subdirectory because anything directly under `tests/` is built
//! as its own test binary; `#[path]`-included from the suites that need it.

#![allow(dead_code)]

use gglib_core::utils::process::cmd;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Output, Stdio};

/// The data root `gglib` runs under for the library in `root`.
pub(crate) fn data_root(root: &Path) -> PathBuf {
    root.join("data")
}

/// Where `gglib` looks for llama.cpp for the library in `root`. Nothing is
/// there unless the test puts it there.
pub(crate) fn resource_root(root: &Path) -> PathBuf {
    root.join("resources")
}

/// A GGUF v3 file in `dir` whose only metadata is its architecture and its
/// file type, `Q8_0`, under its canonical path. The type is the header's to
/// state: gglib never reads it off the name.
pub(crate) fn write_gguf(dir: &Path, name: &str) -> PathBuf {
    use gglib_core::domain::gguf::GgufValue;

    let path = dir.join(name);
    gglib_gguf::write_gguf(
        &path,
        &[
            (
                "general.architecture",
                GgufValue::String("qwen3".to_owned()),
            ),
            ("general.file_type", GgufValue::U32(7)),
        ],
        &[],
        0,
    );
    gglib_core::paths::canonical(&path).expect("its canonical path")
}

/// `gglib <args>` over the library in `root`, with `typed` in the pipe on
/// its input: how it ended, and what it printed. A pipe is not a terminal,
/// so no question reads it: an answer is a flag.
pub(crate) fn run(root: &Path, args: &[&str], typed: &str) -> Output {
    let mut child = cmd(env!("CARGO_BIN_EXE_gglib"))
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
/// file carries no parameter count, so `model add` is given one.
pub(crate) fn library(root: &Path) -> PathBuf {
    let weights = write_gguf(root, "qwen.Q8_0.gguf");
    let file = weights.to_str().unwrap();
    gglib(root, &["model", "add", file, "--param-count", "7"], "");
    weights
}
