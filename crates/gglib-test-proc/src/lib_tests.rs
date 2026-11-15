use std::path::{Path, PathBuf};

use super::{Instruction, executable, instructions_path, parse};

#[test]
fn each_instruction_is_read_from_its_line() {
    let parsed = parse("argv /tmp/a b.txt\nprint out {args}\n\neprint\nsleep 30\nexit 3\n");

    assert_eq!(
        parsed,
        Ok(vec![
            Instruction::Argv(PathBuf::from("/tmp/a b.txt")),
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
