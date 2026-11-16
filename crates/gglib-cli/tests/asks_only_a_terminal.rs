//! A question is asked only of a terminal (#2020).
//!
//! Each command runs as a script or an SSH session runs it: the built
//! binary, with an input that is not a terminal. Such an input is never read
//! for an answer, so a command with a question either takes the answer it
//! can state, and says so, or refuses with one sentence naming the flag that
//! answers it. It never waits, so each run has a deadline, and its output
//! goes to files: `gglib model add` once asked again on every end of input,
//! tens of megabytes a second.

#[path = "support/library.rs"]
mod library;

use gglib_core::domain::gguf::GgufValue;
use gglib_core::utils::process::cmd;
use std::path::{Path, PathBuf};
use std::process::{ChildStdin, Stdio};
use std::time::{Duration, Instant};

use library::{data_root, gglib, library, resource_root, write_gguf};

/// Longer than any of these commands takes, on any runner.
const DEADLINE: Duration = Duration::from_secs(10);

/// What the command's input is.
#[derive(Clone, Copy)]
enum Given {
    /// Closed: the null device, as `< /dev/null` or `< NUL` gives.
    Closed,
    /// A pipe nobody writes to and nobody closes until the command is done,
    /// as an SSH session or a backgrounded shell gives.
    SilentPipe,
    /// A pipe holding this, then closed.
    Piped(&'static str),
}

/// How a run ended.
struct Ended {
    code: Option<i32>,
    stdout: String,
    stderr: String,
}

impl Ended {
    fn text(&self) -> String {
        format!(
            "status: {:?}\nstdout: {}\nstderr: {}",
            self.code, self.stdout, self.stderr
        )
    }
}

/// `gglib <args>` over the library in `root`, with `given` for its input;
/// a panic, with the tail of what it printed, if it is still running at the
/// deadline.
fn run(root: &Path, args: &[&str], given: Given) -> Ended {
    let out = tempfile::tempdir().expect("a folder for the output");
    let (stdout, stderr) = (out.path().join("stdout"), out.path().join("stderr"));
    let stdin = match given {
        Given::Closed => Stdio::null(),
        Given::SilentPipe | Given::Piped(_) => Stdio::piped(),
    };
    let mut child = cmd(env!("CARGO_BIN_EXE_gglib"))
        .args(args)
        .env("GGLIB_DATA_DIR", data_root(root))
        .env("GGLIB_RESOURCE_DIR", resource_root(root))
        .stdin(stdin)
        .stdout(std::fs::File::create(&stdout).expect("its stdout file"))
        .stderr(std::fs::File::create(&stderr).expect("its stderr file"))
        .spawn()
        .unwrap_or_else(|e| panic!("running `gglib {}`: {e}", args.join(" ")));
    // Held until the command is done, so a silent pipe never ends.
    let held: Option<ChildStdin> = match given {
        Given::Closed => None,
        Given::SilentPipe => child.stdin.take(),
        Given::Piped(typed) => {
            let mut pipe = child.stdin.take().expect("its stdin");
            std::io::Write::write_all(&mut pipe, typed.as_bytes()).expect("the pipe is written");
            None
        }
    };

    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().expect("polling gglib") {
            break Some(status);
        }
        if started.elapsed() >= DEADLINE {
            child.kill().expect("stopping gglib");
            child.wait().expect("reaping gglib");
            break None;
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    drop(held);
    let ended = Ended {
        code: status.and_then(|status| status.code()),
        stdout: tail(&stdout),
        stderr: tail(&stderr),
    };
    assert!(
        status.is_some(),
        "`gglib {}` was still waiting after {DEADLINE:?}\n{}",
        args.join(" "),
        ended.text()
    );
    ended
}

/// The last kilobyte of the file at `path`, as text.
fn tail(path: &Path) -> String {
    let bytes = std::fs::read(path).expect("the output file");
    let from = bytes.len().saturating_sub(1024);
    String::from_utf8_lossy(&bytes[from..]).into_owned()
}

/// A GGUF file in `dir` whose header gives a parameter count, 4B, by its
/// size label.
fn write_counted_gguf(dir: &Path) -> PathBuf {
    let path = dir.join("counted.Q8_0.gguf");
    gglib_gguf::write_gguf(
        &path,
        &[
            (
                "general.architecture",
                GgufValue::String("qwen3".to_owned()),
            ),
            ("general.file_type", GgufValue::U32(7)),
            ("general.size_label", GgufValue::String("4B".to_owned())),
        ],
        &[],
        0,
    );
    gglib_core::paths::canonical(&path).expect("its canonical path")
}

/// What `model add` says when it takes the count it read from the file.
const TOOK_THE_FILES_COUNT: &str =
    "Parameter count 4.0B, as the file gives it (no terminal to ask).\n";

/// The question `model add` asks a terminal.
const ASKED: &str = "Parameter count (in billions)";

/// The issue's own case: an SSH session's input is a pipe that stays open.
/// The count the file gives is taken at once, and the command says so.
#[test]
fn model_add_with_a_silent_open_input_takes_the_files_count_at_once() {
    let root = tempfile::tempdir().expect("temp data dir");
    let weights = write_counted_gguf(root.path());

    let added = run(
        root.path(),
        &["model", "add", weights.to_str().unwrap()],
        Given::SilentPipe,
    );

    assert_eq!(added.code, Some(0), "{}", added.text());
    assert!(
        added.stdout.contains(TOOK_THE_FILES_COUNT),
        "{}",
        added.text()
    );
    assert!(
        !added.stdout.contains(ASKED),
        "nobody was asked: {}",
        added.text()
    );
    assert!(
        added.stdout.contains("  Parameters: 4.0B\n"),
        "{}",
        added.text()
    );
}

/// A closed input is the same: the count is taken, and said.
#[test]
fn model_add_with_its_input_closed_says_it_took_the_files_count() {
    let root = tempfile::tempdir().expect("temp data dir");
    let weights = write_counted_gguf(root.path());

    let added = run(
        root.path(),
        &["model", "add", weights.to_str().unwrap()],
        Given::Closed,
    );

    assert_eq!(added.code, Some(0), "{}", added.text());
    assert!(
        added.stdout.contains(TOOK_THE_FILES_COUNT),
        "{}",
        added.text()
    );
}

/// A file that gives no count has no answer to take: the add is refused in
/// one sentence naming the flag, and nothing is added.
#[test]
fn model_add_of_a_file_with_no_count_and_nobody_to_ask_refuses_at_once() {
    let root = tempfile::tempdir().expect("temp data dir");
    let weights = write_gguf(root.path(), "qwen.Q8_0.gguf");
    let file = weights.to_str().unwrap();

    let refused = run(root.path(), &["model", "add", file], Given::Closed);
    let added = gglib(
        root.path(),
        &["model", "add", file, "--param-count", "7"],
        "",
    );

    assert_eq!(refused.code, Some(1), "{}", refused.text());
    assert_eq!(
        refused.stderr,
        "Error: The file gives no parameter count and there is no terminal to ask for one, \
         so the model was not added: run it again with --param-count <billions>.\n",
        "{}",
        refused.text()
    );
    assert!(!refused.stdout.contains(ASKED), "{}", refused.text());
    assert!(
        added.contains("Model successfully added to database!"),
        "{added}"
    );
}

/// `--param-count` answers the question ahead of time, file count or none.
#[test]
fn model_add_takes_the_count_it_is_given() {
    let root = tempfile::tempdir().expect("temp data dir");
    let counted = write_counted_gguf(root.path());
    let uncounted = write_gguf(root.path(), "qwen.Q8_0.gguf");

    for weights in [counted, uncounted] {
        let file = weights.to_str().unwrap();
        let added = run(
            root.path(),
            &["model", "add", file, "--param-count", "7.5"],
            Given::SilentPipe,
        );

        assert_eq!(added.code, Some(0), "{}", added.text());
        assert!(
            added.stdout.contains("  Parameters: 7.5B\n"),
            "{}",
            added.text()
        );
        assert!(!added.stdout.contains(ASKED), "{}", added.text());
        assert!(!added.stdout.contains("no terminal"), "{}", added.text());
    }
}

/// What `model remove` is refused with when nobody is there to confirm it.
const REMOVE_REFUSED: &str = "Error: There is no terminal to ask, so the model was not removed: \
     run it again with --yes to remove it.\n";

/// A removal has no answer to take: nobody agreed, so it is refused naming
/// `--yes`, and the model stays. A pipe holding a yes is not read for one.
#[test]
fn model_remove_with_nobody_to_ask_refuses_and_names_the_flag() {
    let root = tempfile::tempdir().expect("temp data dir");
    library(root.path());

    for given in [Given::Closed, Given::SilentPipe, Given::Piped("y\n")] {
        let refused = run(root.path(), &["model", "remove", "1"], given);

        assert_eq!(refused.code, Some(1), "{}", refused.text());
        assert!(
            refused.stderr.ends_with(REMOVE_REFUSED),
            "{}",
            refused.text()
        );
    }
    let removed = gglib(root.path(), &["model", "remove", "1", "--yes"], "");
    assert_eq!(
        removed,
        "✓ Model 'qwen.Q8_0' (ID 1) successfully removed from database.\n"
    );
}
