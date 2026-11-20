use std::path::{Path, PathBuf};

use super::{BUSY_RETRIES, Instruction, executable, instructions_path, once_not_busy, parse};

#[test]
fn each_instruction_is_read_from_its_line() {
    let parsed = parse(
        "argv /tmp/a b.txt\ncalls /tmp/c d.txt\npid /tmp/p\ntouch /tmp/t\nread\nprint out {args}\n\neprint\nsleep 30\nexit 3\n",
    );

    assert_eq!(
        parsed,
        Ok(vec![
            Instruction::Argv(PathBuf::from("/tmp/a b.txt")),
            Instruction::Calls(PathBuf::from("/tmp/c d.txt")),
            Instruction::Pid(PathBuf::from("/tmp/p")),
            Instruction::Touch(PathBuf::from("/tmp/t")),
            Instruction::Read,
            Instruction::Print("out {args}".to_owned()),
            Instruction::Eprint(String::new()),
            Instruction::Sleep(30),
            Instruction::Exit(3),
        ])
    );
}

#[test]
fn a_line_that_is_no_instruction_is_named() {
    for (text, line) in [
        ("print ok\nsing a song\n", "line 2: \"sing a song\""),
        ("sleep soon", "line 1: \"sleep soon\""),
        ("exit 256", "line 1: \"exit 256\""),
        ("argv", "line 1: \"argv\""),
        ("calls", "line 1: \"calls\""),
        ("pid", "line 1: \"pid\""),
        ("touch", "line 1: \"touch\""),
        ("read all", "line 1: \"read all\""),
    ] {
        assert_eq!(parse(text), Err(line.to_owned()), "{text:?}");
    }
}

#[test]
fn no_instructions_is_an_empty_list() {
    assert_eq!(parse(""), Ok(Vec::new()));
}

/// The rule the program and `install` share: the OS's executable suffix
/// comes off, `.proc` goes on, and any other dot in the name stays.
#[test]
fn the_instruction_file_is_named_after_the_program_without_its_suffix() {
    let suffix = std::env::consts::EXE_SUFFIX;
    let dir = Path::new("some").join("dir");
    for (program, file) in [
        (format!("llama-server{suffix}"), "llama-server.proc"),
        ("llama-server".to_owned(), "llama-server.proc"),
        (format!("sd-server.v2{suffix}"), "sd-server.v2.proc"),
    ] {
        assert_eq!(instructions_path(&dir.join(program)), dir.join(file));
    }
}

#[test]
fn cargos_report_names_the_executable() {
    let unix = r#"{"reason":"compiler-artifact","target":{"name":"gglib-test-proc"},"executable":"/w/target/debug/gglib-test-proc","fresh":true}"#;
    let windows = r#"{"reason":"compiler-artifact","executable":"C:\\w\\target\\debug\\gglib-test-proc.exe","fresh":false}"#;
    let escaped = r#"{"executable":"/w/a \"q\"/\u00e9\/x","fresh":true}"#;

    assert_eq!(
        executable(unix),
        Some(PathBuf::from("/w/target/debug/gglib-test-proc"))
    );
    assert_eq!(
        executable(windows),
        Some(PathBuf::from(r"C:\w\target\debug\gglib-test-proc.exe"))
    );
    assert_eq!(
        executable(escaped),
        Some(PathBuf::from("/w/a \"q\"/\u{e9}/x"))
    );
}

#[test]
fn a_message_without_an_executable_names_none() {
    for line in [
        r#"{"reason":"compiler-artifact","executable":null,"fresh":true}"#,
        r#"{"reason":"build-script-executed","out_dir":"/w/out"}"#,
        r#"{"reason":"compiler-artifact","executable":"/w/unclosed"#,
        r#"{"executable":"/w/\q"}"#,
        "",
    ] {
        assert_eq!(executable(line), None, "{line}");
    }
}

/// A program the kernel calls busy is run again until a run starts, and what
/// that run answers is handed back.
#[test]
fn a_busy_program_is_run_again_until_it_starts() {
    let mut runs = 0;

    let outcome = once_not_busy(|| {
        runs += 1;
        if runs < 3 {
            Err(std::io::ErrorKind::ExecutableFileBusy.into())
        } else {
            Ok(runs)
        }
    });

    assert_eq!(outcome.expect("the third run starts"), 3);
}

/// Only a busy program is waited for: any other failure is the answer.
#[test]
fn a_program_that_fails_for_another_reason_is_not_run_again() {
    let mut runs = 0;

    let outcome = once_not_busy(|| -> std::io::Result<()> {
        runs += 1;
        Err(std::io::ErrorKind::NotFound.into())
    });

    assert_eq!(
        outcome.expect_err("it does not start").kind(),
        std::io::ErrorKind::NotFound
    );
    assert_eq!(runs, 1);
}

/// A program that stays busy is not waited for without end: after the last
/// retry its refusal is the answer.
#[test]
fn a_program_that_stays_busy_is_given_up_on() {
    let mut runs = 0;

    let outcome = once_not_busy(|| -> std::io::Result<()> {
        runs += 1;
        Err(std::io::ErrorKind::ExecutableFileBusy.into())
    });

    assert_eq!(
        outcome.expect_err("it never starts").kind(),
        std::io::ErrorKind::ExecutableFileBusy
    );
    assert_eq!(runs, BUSY_RETRIES + 1);
}

/// A copy another thread still has open for writing is not handed back
/// until that thread lets go of it. macOS runs such a file, and there is
/// nothing to wait out there, so the test ends early.
// Unix only: only Linux refuses to start a file open for writing.
#[cfg(unix)]
#[test]
fn a_copy_still_open_for_writing_is_waited_out() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    let dir = tempfile::tempdir().unwrap();
    let program = dir.path().join("server");
    std::fs::copy(super::built(), &program).unwrap();
    let held = std::fs::OpenOptions::new()
        .append(true)
        .open(&program)
        .unwrap();
    let refused = super::command(&program)
        .stdin(std::process::Stdio::null())
        .output();
    if !matches!(&refused, Err(e) if e.kind() == std::io::ErrorKind::ExecutableFileBusy) {
        return;
    }
    let let_go = Arc::new(AtomicBool::new(false));
    let holder = std::thread::spawn({
        let let_go = Arc::clone(&let_go);
        move || {
            std::thread::sleep(std::time::Duration::from_millis(200));
            let_go.store(true, Ordering::SeqCst);
            drop(held);
        }
    });

    super::run_once(&program);

    assert!(
        let_go.load(Ordering::SeqCst),
        "handed back while still open for writing"
    );
    holder.join().unwrap();
}
