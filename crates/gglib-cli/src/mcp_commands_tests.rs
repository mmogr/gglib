//! `gglib mcp add`'s `--args` as clap parses it.
//!
//! A server's arguments are mostly flags (`npx -y pkg`, the help's own
//! `--port,3000`), so the option's value has to be allowed to start with a
//! dash; it was refused with "unexpected argument '-y'" (C12 in #1959).

use clap::Parser as _;

use super::*;

/// The name and arguments `flags` parse to on `gglib mcp add --command npx`.
fn parsed(flags: &[&str]) -> Result<(String, Vec<String>), clap::Error> {
    let argv = ["gglib", "mcp", "add", "--command", "npx"]
        .iter()
        .chain(flags);
    let cli = crate::Cli::try_parse_from(argv)?;
    let Some(crate::Commands::Mcp {
        command: McpCommand::Add { name, args, .. },
    }) = cli.command
    else {
        panic!("`mcp add` did not parse as an add");
    };
    Ok((name, args))
}

#[test]
fn arguments_that_start_with_a_dash_are_the_servers() {
    let cases: &[(&[&str], &[&str])] = &[
        (
            &[
                "--name",
                "x",
                "--args",
                "-y,@modelcontextprotocol/server-everything",
            ],
            &["-y", "@modelcontextprotocol/server-everything"],
        ),
        (
            &["--name", "x", "--args", "--port,3000"],
            &["--port", "3000"],
        ),
        (&["--args", "-y", "--name", "x"], &["-y"]),
        (&["--name", "x", "--args=-y,pkg"], &["-y", "pkg"]),
        (&["--name", "x", "--args", "pkg"], &["pkg"]),
        (&["--name", "x"], &[]),
    ];
    for (flags, expected) in cases {
        let (name, args) = parsed(flags).unwrap_or_else(|e| panic!("{flags:?}: {e}"));
        assert_eq!(name, "x", "{flags:?}");
        assert_eq!(args, *expected, "{flags:?}");
    }
}
