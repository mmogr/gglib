//! The command line as a function of the plan and the shell it is written
//! for. Every case runs on every OS: the shell is a value, not the machine
//! the test runs on. The last test runs the lines through the real shells
//! this machine has, which is the only proof a quoting rule is right.

use std::path::Path;

use super::*;
use crate::host::{HostOs, Shell};

fn arg(flag: &str, value: Option<&str>) -> LaunchArg {
    LaunchArg {
        flag: flag.to_owned(),
        value: value.map(str::to_owned),
        decision: "model".to_owned(),
        source: ChoiceSource::Gglib,
        reason: String::new(),
    }
}

fn plan_of(program: &str, args: Vec<LaunchArg>) -> LaunchPlan {
    LaunchPlan {
        program: program.to_owned(),
        args,
        narration: LaunchNarration::new("m", None, 0),
        unloads: None,
        fit: None,
    }
}

fn plan(args: Vec<LaunchArg>) -> LaunchPlan {
    plan_of("llama-server", args)
}

/// `value` as the line for `shell` writes it, after `--v `.
fn token(shell: Shell, value: &str) -> String {
    let line = plan(vec![arg("--v", Some(value))]).command_line(shell).line;
    line.strip_prefix("llama-server --v ")
        .unwrap_or_else(|| panic!("{line}"))
        .to_owned()
}

#[test]
fn the_command_line_is_the_program_and_each_token_in_order() {
    let plan = plan(vec![
        arg("-m", Some("/models/qwen.gguf")),
        arg("--metrics", None),
        arg("--port", Some("auto")),
    ]);
    for shell in [Shell::Posix, Shell::PowerShell] {
        let command = plan.command_line(shell);
        assert_eq!(command.shell, shell);
        assert_eq!(command.program, "llama-server");
        assert_eq!(
            command.args,
            ["-m /models/qwen.gguf", "--metrics", "--port auto"]
        );
        assert_eq!(
            command.line,
            "llama-server -m /models/qwen.gguf --metrics --port auto"
        );
    }
}

#[test]
fn a_posix_line_quotes_a_value_in_single_quotes() {
    let shell = Shell::Posix;
    // A path with spaces.
    assert_eq!(
        token(shell, "/Users/m/Library/Application Support/g/a.gguf"),
        "'/Users/m/Library/Application Support/g/a.gguf'"
    );
    // A backslash is a POSIX shell's escape: quoted.
    assert_eq!(token(shell, r"C:\Users\m\q.gguf"), r"'C:\Users\m\q.gguf'");
    // A quote inside a value closes, escapes and reopens.
    assert_eq!(token(shell, "/it's/p.gguf"), r"'/it'\''s/p.gguf'");
    // An empty value is still a token.
    assert_eq!(token(shell, ""), "''");
    // A leading dash means nothing to the shell.
    assert_eq!(token(shell, "-x.y"), "-x.y");
    // What a shell would expand, quoted; what it reads as itself, left alone.
    assert_eq!(token(shell, "$HOME"), "'$HOME'");
    assert_eq!(
        token(shell, r"blk\.\d+\.ffn_.*=CPU"),
        r"'blk\.\d+\.ffn_.*=CPU'"
    );
    assert_eq!(token(shell, "k=v,a+b%c:d@e"), "k=v,a+b%c:d@e");
    assert_eq!(token(shell, "127.0.0.1"), "127.0.0.1");
}

#[test]
fn a_powershell_line_quotes_a_value_in_powershell_single_quotes() {
    let shell = Shell::PowerShell;
    // A path with spaces.
    assert_eq!(
        token(shell, r"C:\Users\m\AppData\Local\gglib\a b.gguf"),
        r"'C:\Users\m\AppData\Local\gglib\a b.gguf'"
    );
    // A backslash is no escape in PowerShell: a Windows path is left alone.
    assert_eq!(
        token(shell, r"C:\Users\m\q.Q8_0.gguf"),
        r"C:\Users\m\q.Q8_0.gguf"
    );
    // A quote inside a value is doubled, the typographic ones too, which
    // PowerShell also reads as quotes.
    assert_eq!(token(shell, r"C:\it's\p.gguf"), r"'C:\it''s\p.gguf'");
    assert_eq!(token(shell, "it\u{2019}s"), "'it\u{2019}\u{2019}s'");
    // An empty value is still a token.
    assert_eq!(token(shell, ""), "''");
    // A bare `-x.y` is split by PowerShell into `-x` and `.y`: quoted.
    assert_eq!(token(shell, "-x.y"), "'-x.y'");
    // A short name's `~` is itself; a leading one is a home folder.
    assert_eq!(
        token(shell, r"C:\Users\RUNNER~1\q.gguf"),
        r"C:\Users\RUNNER~1\q.gguf"
    );
    assert_eq!(token(shell, r"~\q.gguf"), r"'~\q.gguf'");
    // `$` expands, `,` makes a list, `@` splats, `*` and `?` are wildcards.
    assert_eq!(token(shell, "$HOME"), "'$HOME'");
    assert_eq!(token(shell, "k=v,a+b%c:d@e"), "'k=v,a+b%c:d@e'");
    assert_eq!(
        token(shell, r"blk\.\d+\.ffn_.*=CPU"),
        r"'blk\.\d+\.ffn_.*=CPU'"
    );
    assert_eq!(token(shell, r"\\?\C:\m\q.gguf"), r"'\\?\C:\m\q.gguf'");
    // A double quote is literal inside single quotes.
    assert_eq!(token(shell, r#"say "hi""#), r#"'say "hi"'"#);
    assert_eq!(token(shell, "127.0.0.1"), "127.0.0.1");
    assert_eq!(token(shell, "0.95"), "0.95");
    assert_eq!(token(shell, "08"), "08");
    assert_eq!(token(shell, "1e3"), "1e3");
    // A folder with a space keeps its last backslash inside the quotes.
    assert_eq!(token(shell, r"D:\My Cache\"), r"'D:\My Cache\'");
}

#[test]
fn a_program_that_needs_quoting_is_quoted_and_powershell_calls_it_with_an_ampersand() {
    let posix = plan_of("/Applications/My Apps/llama-server", vec![]);
    assert_eq!(
        posix.command_line(Shell::Posix).line,
        "'/Applications/My Apps/llama-server'"
    );
    let windows = plan_of(r"C:\Program Files\gglib\llama-server.exe", vec![]);
    assert_eq!(
        windows.command_line(Shell::PowerShell).line,
        r"& 'C:\Program Files\gglib\llama-server.exe'"
    );
    assert_eq!(
        plan(vec![]).command_line(Shell::PowerShell).line,
        "llama-server"
    );
}

/// The values the round trip sends, each a case the unit tests above name.
const VALUES: [&str; 17] = [
    "/Users/m/Library/Application Support/g/a.gguf",
    r"C:\Users\m\AppData\Local\gglib\a b.gguf",
    r"C:\Users\m\q.Q8_0.gguf",
    r"C:\Users\RUNNER~1\q.gguf",
    r"~\q.gguf",
    "/it's/p.gguf",
    "it\u{2019}s",
    "-x.y",
    "$HOME",
    r"blk\.\d+\.ffn_.*=CPU",
    "k=v,a+b%c:d@e",
    "0.95",
    "08",
    "1e3",
    "",
    r#"say "hi""#,
    r"D:\My Cache\",
];

/// What Windows PowerShell 5.1 does not pass to a program as written, in
/// any quoting: it drops an empty argument, does not escape a double quote
/// inside one, and does not escape the backslash that ends one with a
/// space. PowerShell 7.3 and later pass all three.
const WINDOWS_POWERSHELL_LOSES: [&str; 3] = ["", r#"say "hi""#, r"D:\My Cache\"];

/// The arguments a program received when `interpreter` ran the line `shell`
/// writes for `values`, or `None` when this machine has no such program.
fn run_through(
    dir: &Path,
    interpreter: &str,
    shell: Shell,
    values: &[&str],
) -> Option<Vec<String>> {
    let received = dir.join(format!("{interpreter}.argv"));
    // In a folder with a space, so the program itself is quoted (and, in
    // PowerShell, called with `&`).
    let folder = dir.join("with space");
    std::fs::create_dir_all(&folder).unwrap();
    let program = gglib_test_proc::install(
        &folder,
        &format!("echo-{interpreter}"),
        &format!("argv {}", received.display()),
    );
    let sent = values.iter().map(|v| arg("--v", Some(v))).collect();
    let line = plan_of(&program.to_string_lossy(), sent)
        .command_line(shell)
        .line;
    let (script, text, flags): (_, _, &[&str]) = match shell {
        Shell::Posix => (dir.join("line.sh"), line.clone(), &[]),
        // A byte-order mark, or Windows PowerShell reads the file in the
        // machine's code page.
        Shell::PowerShell => (
            dir.join(format!("line-{interpreter}.ps1")),
            format!("\u{feff}{line}\n"),
            &[
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-File",
            ],
        ),
    };
    std::fs::write(&script, text).unwrap();
    let output = match crate::utils::process::cmd(interpreter)
        .args(flags)
        .arg(&script)
        .output()
    {
        Ok(output) => output,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return None,
        Err(e) => panic!("{interpreter}: {e}"),
    };
    assert!(
        output.status.success(),
        "{interpreter} ran {line}: {output:?}"
    );
    let got = std::fs::read_to_string(&received)
        .unwrap_or_else(|e| panic!("{interpreter} ran {line}, and no argv: {e} {output:?}"));
    Some(got.lines().map(str::to_owned).collect())
}

/// The line this machine writes, run by its own shell, hands the program
/// exactly the plan's values; and where PowerShell 7 is installed (every
/// GitHub runner has it), the PowerShell line run by it does too.
#[test]
fn a_line_run_by_its_shell_gives_the_program_the_plans_values() {
    let dir = tempfile::tempdir().unwrap();
    let host = HostOs::current().shell();
    let mut runs = vec![(
        match host {
            Shell::Posix => "sh",
            Shell::PowerShell => "powershell",
        },
        host,
        true,
    )];
    runs.push(("pwsh", Shell::PowerShell, false));
    for (interpreter, shell, required) in runs {
        let values: Vec<&str> = VALUES
            .into_iter()
            .filter(|v| interpreter != "powershell" || !WINDOWS_POWERSHELL_LOSES.contains(v))
            .collect();
        let Some(got) = run_through(dir.path(), interpreter, shell, &values) else {
            assert!(!required, "this machine has no {interpreter}");
            continue;
        };
        let want: Vec<String> = values
            .iter()
            .flat_map(|v| ["--v".to_owned(), (*v).to_owned()])
            .collect();
        assert_eq!(got, want, "through {interpreter}");
    }
}
