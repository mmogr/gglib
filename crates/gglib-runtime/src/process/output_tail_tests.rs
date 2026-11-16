//! The rule for the line that says why a load failed, held to what each
//! engine really printed (`output_tail_fixtures.rs`).

use super::*;

#[path = "output_tail_fixtures.rs"]
mod recorded;

fn lines(text: &[&str]) -> Vec<String> {
    text.iter().map(|line| (*line).to_owned()).collect()
}

/// The line `printed` (one recording) is said to fail with.
fn said(printed: &str) -> String {
    let printed: Vec<String> = printed.lines().map(str::to_owned).collect();
    telling_line(&printed).expect("a line")
}

/// Each recording of a llama-server that failed: the line said is the
/// cause, the first of the last run of errors. The last error line of each
/// that has one is a farewell that names no cause, which is why the last
/// error line is not the rule.
#[test]
fn llama_cpp_failures_say_their_cause() {
    let cases = [
        (
            recorded::NOT_A_GGUF,
            "gguf_init_from_reader: invalid magic characters: 'this', expected 'GGUF'",
        ),
        (
            recorded::MISSING_FILE,
            "gguf_init_from_file: failed to open GGUF file '/nonexistent/qwen.gguf' (No such file \
             or directory)",
        ),
        (
            recorded::MISSING_MMPROJ,
            "gguf_init_from_file: failed to open GGUF file '/nonexistent/mmproj.gguf' (No such \
             file or directory)",
        ),
        (
            recorded::PORT_IN_USE,
            "srv         start: couldn't bind HTTP server socket, hostname: 127.0.0.1, port: 9887",
        ),
        (recorded::BAD_FLAG, "error: invalid argument: --bogus-flag"),
    ];
    for (printed, cause) in cases {
        assert_eq!(said(printed), cause);
    }
    for printed in [
        recorded::NOT_A_GGUF,
        recorded::MISSING_FILE,
        recorded::MISSING_MMPROJ,
    ] {
        let last_error = printed.lines().rfind(|line| line.contains(" E ")).unwrap();
        assert!(
            last_error.ends_with("exiting due to model loading error"),
            "{last_error}"
        );
    }
    let last_error = recorded::PORT_IN_USE
        .lines()
        .rfind(|line| line.contains(" E "))
        .unwrap();
    assert!(last_error.ends_with("exiting due to HTTP server error"));
}

/// The missing projector is reported twice: once early, by a probe the
/// server carries on after, and again as the load fails. The line said is
/// from the second run, the one the server died of: the early errors are
/// cut off from it by the model's own loading.
#[test]
fn an_early_error_the_server_carried_on_from_is_not_the_cause() {
    let early = "0.00.090.385 E gguf_init_from_file";
    let late = "0.07.513.402 E gguf_init_from_file";
    assert!(recorded::MISSING_MMPROJ.contains(early));
    assert!(recorded::MISSING_MMPROJ.contains(late));

    // The run said begins at the late line: what is before it is ordinary.
    let printed: Vec<&str> = recorded::MISSING_MMPROJ
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect();
    let at = printed
        .iter()
        .position(|line| line.starts_with(late))
        .unwrap();
    assert!(!printed[at - 1].contains(" E "), "{}", printed[at - 1]);
    assert!(!printed[at - 2].contains(" E "), "{}", printed[at - 2]);

    // And an early error alone, followed by ordinary lines, is not said.
    let invented = lines(&[
        "0.00.001.000 E load_backend: failed to load an optional backend",
        "0.00.002.000 I load_tensors: loading model tensors",
        "0.00.003.000 I llama_context: n_ctx = 4096",
        "0.00.004.000 E llama_model_load: error loading model: out of memory",
        "0.00.004.100 I srv    operator(): operator(): cleaning up before exit...",
        "0.00.004.200 E srv  server_run_i: exiting due to model loading error",
    ]);
    assert_eq!(
        telling_line(&invented).as_deref(),
        Some("llama_model_load: error loading model: out of memory")
    );
}

/// sd-server writes its warnings on stdout and its errors on stderr; its
/// `[W]` lines are not errors, and its errors' run begins with the cause.
#[test]
fn stable_diffusion_cpp_says_its_cause() {
    assert_eq!(
        said(recorded::SD_NOT_A_GGUF),
        "get sd version from file failed: '' --- diffusion_engine.cpp:972"
    );
}

/// A llama-server still loading when stopped printed no error: its last
/// line, a warning, is said.
#[test]
fn a_server_still_loading_says_its_last_line() {
    assert_eq!(
        said(recorded::STILL_LOADING),
        "llama_context: n_ctx_seq (2000128) > n_ctx_train (262144) -- possible training context \
         overflow"
    );
}

/// A server still loading has printed no error: its last line says where
/// it was, even when an earlier line was a warning with an error word.
#[test]
fn with_no_error_the_last_line_is_given() {
    let marked = lines(&[
        "0.00.100.000 W load: failed to read the chat template, using the default",
        "0.00.200.000 I load_tensors: loading model tensors, this can take a while...",
    ]);
    let unmarked = lines(&["loading", "still loading", "   "]);

    assert_eq!(
        telling_line(&marked).as_deref(),
        Some("load_tensors: loading model tensors, this can take a while...")
    );
    assert_eq!(telling_line(&unmarked).as_deref(), Some("still loading"));
    assert_eq!(telling_line(&lines(&["", "  "])), None);
    assert_eq!(telling_line(&[]), None);
}

/// A long line is cut by characters, not bytes, so a line of lossy UTF-8
/// cannot be cut inside a character.
#[test]
fn a_long_line_is_cut_by_characters() {
    let long = format!("error: {}", "é".repeat(SAID_CHARS * 2));

    let said = telling_line(&[long]).expect("a line");

    assert_eq!(said.chars().count(), SAID_CHARS);
    assert!(said.ends_with('…'), "{said}");
}

/// Only the last lines are kept, so a server that prints for hours holds
/// a bounded amount.
#[test]
fn only_the_last_lines_are_kept() {
    let tail = OutputTail::default();
    for n in 0..KEPT_LINES + 10 {
        tail.push(&format!("line {n}"));
    }

    let kept = tail.lines();

    assert_eq!(kept.len(), KEPT_LINES);
    assert_eq!(kept.first().map(String::as_str), Some("line 10"));
}

/// Settling waits for every reader to reach its stream's end, and no
/// longer than it is given when one never does.
#[tokio::test]
async fn settling_waits_for_the_readers_and_is_bounded() {
    let tail = OutputTail::default();
    tail.opened();
    tail.opened();
    let reader = tail.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(50)).await;
        reader.push("last words");
        reader.closed();
        reader.closed();
    });

    tail.settled(Duration::from_secs(10)).await;
    assert_eq!(tail.lines(), ["last words"]);

    let held = OutputTail::default();
    held.opened();
    let started = std::time::Instant::now();
    held.settled(Duration::from_millis(100)).await;
    assert!(started.elapsed() < Duration::from_secs(5));
}

/// Nothing an engine prints reaches a sentence as a terminal control: a
/// model file's string can be in a line (llama.cpp prints an architecture
/// it does not know), and a colour setting the user exported reaches the
/// engine. Escape sequences go whole, a tab is a space, every other control
/// character goes.
#[test]
fn a_kept_line_holds_no_terminal_control() {
    let tail = OutputTail::default();
    // CSI: clear the screen, then colour.
    tail.push("0.00.001.000 E llama_model_load: unknown model architecture: '\u{1b}[2J\u{1b}[31mx\u{1b}[0m'");
    // OSC 52, a clipboard write, ended by BEL; and one ended by ST.
    tail.push("E arch '\u{1b}]52;c;cm0gLXJmIH4=\u{7}' and title \u{1b}]0;owned\u{1b}\\ done");
    // A carriage return that would overwrite the line, a tab, a NUL.
    tail.push("real\rfake\tafter\u{0}end");
    // A C1 CSI.
    tail.push("one\u{9b}2Jtwo");

    let kept = tail.lines();

    assert_eq!(
        kept,
        [
            "0.00.001.000 E llama_model_load: unknown model architecture: 'x'",
            "E arch '' and title  done",
            "realfake afterend",
            "onetwo",
        ]
    );
    for line in &kept {
        assert!(!line.chars().any(char::is_control), "{line:?}");
    }
}

/// llama.cpp coloured by `LLAMA_ARG_LOG_COLORS=on`: once the colours are
/// gone its level marks are read, and the cause is found as uncoloured.
#[test]
fn a_coloured_line_keeps_its_level_mark() {
    let tail = OutputTail::default();
    tail.push("\u{1b}[32m0.00.001.000 I srv    load_model: loading model\u{1b}[0m");
    tail.push("\u{1b}[31m0.00.002.000 E gguf_init_from_file: failed to open GGUF file\u{1b}[0m");
    tail.push("\u{1b}[32m0.00.003.000 I srv    operator(): cleaning up before exit...\u{1b}[0m");
    tail.push(
        "\u{1b}[31m0.00.004.000 E srv  server_run_i: exiting due to model loading error\u{1b}[0m",
    );

    assert_eq!(
        tail.said().as_deref(),
        Some("gguf_init_from_file: failed to open GGUF file")
    );
}

/// The first line an engine wrote when asked `--version`, as its status
/// shows it, is made plain and cut as a kept line is.
#[test]
fn the_checks_first_line_is_plain_and_bounded() {
    let mut printed = b"\n  \x1b]52;c;cm0=\x07error: \x1b[1mbad\x1b[0m\r".to_vec();
    printed.extend(std::iter::repeat_n(b'x', SAID_CHARS * 2));

    let line = crate::engine::first_line(&printed).expect("a line");

    assert!(line.starts_with("error: bad"), "{line:?}");
    assert!(!line.chars().any(char::is_control), "{line:?}");
    assert_eq!(line.chars().count(), SAID_CHARS);
}
