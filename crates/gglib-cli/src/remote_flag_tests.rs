//! Tests for [`super`]: where `--remote` parses, and what it is told where
//! it does not.

use clap::CommandFactory as _;

use super::*;
use crate::target::REACHES;

fn remote(argv: &[&str]) -> bool {
    match parse_from(argv) {
        Ok((_, remote)) => remote,
        Err(refusal) => panic!("`{}` was refused: {refusal:?}", argv.join(" ")),
    }
}

fn refused(argv: &[&str]) -> String {
    match parse_from(argv) {
        Err(Refusal::Sentence(sentence)) => sentence,
        Err(Refusal::Clap(error)) => panic!("`{}` got clap's error: {error}", argv.join(" ")),
        Ok(_) => panic!("`{}` parsed", argv.join(" ")),
    }
}

/// The flag parses after each command that reaches the paired machine, and
/// says so; without it the same command is for this machine.
#[test]
fn each_command_that_reaches_the_paired_machine_takes_the_flag() {
    for argv in [
        &["gglib", "chat", "--remote"][..],
        &["gglib", "q", "--remote", "hi"],
        &["gglib", "serve", "7", "--remote"],
        &["gglib", "model", "list", "--remote"],
        &["gglib", "model", "inspect", "7", "--remote"],
        &["gglib", "proxy", "dashboard", "--remote"],
        &["gglib", "proxy", "cache-clear", "--remote"],
        &["gglib", "daemon", "stop", "--remote"],
        &["gglib", "image", "a cat", "--remote"],
    ] {
        assert!(remote(argv), "{}", argv.join(" "));
        let without: Vec<&str> = argv.iter().copied().filter(|a| *a != "--remote").collect();
        assert!(!remote(&without), "{}", without.join(" "));
    }
}

/// A command that is about this machine refuses the flag with ADR 0013's
/// sentence, word for word, not with clap's "unexpected argument".
#[test]
fn a_command_about_this_machine_refuses_the_flag_with_the_one_sentence() {
    let text = refused(&["gglib", "attachment", "save", "3f9a2c1e", "--remote"]);
    assert_eq!(
        text,
        format!(
            "`gglib attachment` is about this machine, and --remote does not change that. \
             What --remote reaches on the paired machine: {}.",
            *REACHES
        )
    );
    let text = refused(&["gglib", "model", "remove", "3", "--remote"]);
    assert!(
        text.starts_with("`gglib model` is about this machine"),
        "{text}"
    );
}

/// The commands with a reason of their own keep it.
#[test]
fn a_command_with_its_own_reason_keeps_its_sentence() {
    let text = refused(&["gglib", "proxy", "stop", "--remote"]);
    assert!(text.starts_with("`gglib proxy stop` --remote:"), "{text}");
    assert!(text.contains("gglib daemon stop --remote"), "{text}");
    let text = refused(&["gglib", "web", "--remote"]);
    assert!(
        text.contains("already lists the paired machine's models"),
        "{text}"
    );
}

/// Before the command it belongs to, the flag is not ignored and not
/// refused as if the command were local: the line says where it goes.
#[test]
fn the_flag_before_its_command_is_told_where_it_goes() {
    let text = refused(&["gglib", "--remote", "chat"]);
    assert!(text.contains("goes after the command"), "{text}");
    assert!(text.contains("`gglib chat … --remote`"), "{text}");
    let text = refused(&["gglib", "proxy", "--remote", "dashboard"]);
    assert!(
        text.contains("`gglib proxy dashboard … --remote`"),
        "{text}"
    );
}

/// A line that names no command, or does not parse without the flag
/// either, gets clap's own error, which says what is actually wrong.
#[test]
fn a_line_with_nothing_to_refuse_gets_clap_s_error() {
    for argv in [
        &["gglib", "--remote"][..],
        &["gglib", "model", "remove", "--remote"],
    ] {
        assert!(
            matches!(parse_from(argv), Err(Refusal::Clap(_))),
            "{}",
            argv.join(" ")
        );
    }
}

/// The flags `chat` and `q` no longer take, since the daemon runs their
/// turns, are refused by name rather than taken and ignored.
#[test]
fn the_flags_of_the_in_process_loop_are_refused_on_both_agent_commands() {
    for flag in [
        &["--port", "9000"][..],
        &["--observation-tool", "read_file"],
        &["--max-observation-steps", "3"],
        &["--context-size", "8192"],
        &["-c", "8192"],
        &["--mlock"],
    ] {
        for command in [&["gglib", "q", "hi"][..], &["gglib", "chat", "m"]] {
            let argv = [command, flag].concat();
            assert!(
                matches!(parse_from(&argv), Err(Refusal::Clap(_))),
                "{}",
                argv.join(" ")
            );
        }
    }
    for chat_only in [&["--no-retry"][..], &["--model", "qwen"]] {
        let argv = [&["gglib", "chat", "m"][..], chat_only].concat();
        assert!(
            matches!(parse_from(&argv), Err(Refusal::Clap(_))),
            "{}",
            argv.join(" ")
        );
    }
}

/// The root's own help offers no `--remote`: there is nothing at the root
/// for it to do.
#[test]
fn the_root_does_not_offer_the_flag() {
    let root = Cli::command();
    assert!(root.get_arguments().all(|arg| arg.get_id() != REMOTE));
}

/// Every command the parser knows, as its name and a command line that
/// parses: its required arguments, and the first of each group it requires
/// one of, filled with the first value each takes. A parent that cannot run
/// without a subcommand is not a line of its own.
fn every_command_line() -> Vec<(String, Vec<String>)> {
    fn walk(command: &clap::Command, path: &mut Vec<String>, out: &mut Vec<(String, Vec<String>)>) {
        for sub in command.get_subcommands() {
            path.push(sub.get_name().to_owned());
            if !sub.is_subcommand_required_set() {
                let mut argv = vec!["gglib".to_owned()];
                argv.extend(path.iter().cloned());
                let first_of_each_group: Vec<&clap::Id> = sub
                    .get_groups()
                    .filter(|group| group.is_required_set())
                    .filter_map(|group| group.get_args().next())
                    .collect();
                let required = sub.get_arguments().filter(|arg| {
                    arg.is_required_set() || first_of_each_group.contains(&arg.get_id())
                });
                for arg in required {
                    let value = arg
                        .get_possible_values()
                        .first()
                        .map_or_else(|| "7".to_owned(), |v| v.get_name().to_owned());
                    if !arg.is_positional() {
                        argv.push(format!("--{}", arg.get_long().expect("a named flag")));
                    }
                    argv.push(value);
                }
                out.push((path.join(" "), argv));
            }
            walk(sub, path, out);
            path.pop();
        }
    }
    let mut out = Vec::new();
    walk(&Cli::command(), &mut Vec::new(), &mut out);
    out
}

/// The parser and ADR 0013's table say the same thing about every command:
/// it declares `--remote` exactly when [`reach`] lets it use a machine.
/// The `model` rows of that table are core's `ModelAction::on_paired()`
/// (`target_tests`), so the parser agrees with the web page's table too.
#[test]
fn a_command_takes_the_flag_exactly_when_it_uses_a_machine() {
    let declared = declared_on();
    let lines = every_command_line();
    assert!(lines.len() > 40, "the walk found {} commands", lines.len());
    for (path, argv) in lines {
        let (cli, _) = parse_from(&argv)
            .unwrap_or_else(|refusal| panic!("`{}` did not parse: {refusal:?}", argv.join(" ")));
        let (name, how) = reach(&cli.command.expect("a subcommand"));
        assert_eq!(
            declared.contains(&path),
            how == Reach::Use,
            "`gglib {path}` (reach names it `{name}`, {how:?}) and the parser disagree about --remote"
        );
        if how == Reach::Use {
            assert_eq!(
                name, path,
                "the refusal and the parser name `{path}` differently"
            );
        }
    }
}

/// A flag the paired machine would never receive is refused by name beside
/// `--remote`, in the one sentence, and only the flags typed are named.
#[test]
fn a_flag_remote_would_not_carry_is_refused_by_name() {
    assert_eq!(
        refused(&[
            "gglib",
            "serve",
            "7",
            "--remote",
            "--temperature",
            "0.5",
            "--port",
            "9000"
        ]),
        "--remote has the paired machine load the model, and only its name and --context-size \
         travel, so --port and --temperature would not reach it; drop them"
    );
    for flag in [
        &["--mlock"][..],
        &["--jinja", "on"],
        &["--host", "0.0.0.0"],
        &["--profile", "coding"],
        &["--cache"],
        &["--api-key", "k"],
        &["--mtp-draft-n-max", "2"],
    ] {
        let argv = [&["gglib", "serve", "7", "--remote"][..], flag].concat();
        let text = refused(&argv);
        assert!(text.contains(&format!("so {} would", flag[0])), "{text}");
    }
    // Refused because typed, not for the value: `added` and `desc` are the
    // defaults, typed out.
    for flag in [
        &["--sort", "added"][..],
        &["--order", "desc"],
        &["--tag", "chat"],
        &["--min-params", "1"],
        &["--max-params", "9"],
        &["--min-speed", "1"],
        &["--max-speed", "9"],
    ] {
        let argv = [&["gglib", "model", "list", "--remote"][..], flag].concat();
        assert_eq!(
            refused(&argv),
            format!(
                "--remote lists the paired machine's models as that machine sends them, so {} \
                 would sort and filter nothing; drop it",
                flag[0]
            )
        );
    }
    for client in ["dashboard", "cache-clear"] {
        assert_eq!(
            refused(&[
                "gglib",
                "proxy",
                client,
                "--remote",
                "--host",
                "127.0.0.1",
                "--port",
                "9887",
                "--api-key",
                "k",
            ]),
            "--remote names the paired machine's proxy, so --host, --port and --api-key would \
             name a different one; drop them",
            "{client}"
        );
    }
}

/// What does travel is not refused.
#[test]
fn what_remote_carries_is_not_refused() {
    for argv in [
        &["gglib", "serve", "7", "--remote", "--context-size", "8192"][..],
        &["gglib", "serve", "7", "-c", "8192", "--remote", "-v"],
        &[
            "gglib",
            "proxy",
            "cache-clear",
            "--remote",
            "--session-id",
            "s",
        ],
    ] {
        assert!(remote(argv), "{}", argv.join(" "));
    }
}

/// Each command [`CARRIED`] names declares `--remote`, and each argument it
/// says travels is one that command takes: a renamed command or argument
/// would otherwise leave the table refusing nothing, or the one that travels.
#[test]
fn the_carried_table_names_real_commands_and_arguments() {
    let declared = declared_on();
    let root = Cli::command();
    for (path, travels, ..) in CARRIED {
        assert!(
            declared.iter().any(|d| d == path),
            "`{path}` takes no --remote"
        );
        let mut command = &root;
        for name in path.split(' ') {
            command = command.find_subcommand(name).expect(path);
        }
        for id in travels {
            assert!(
                command.get_arguments().any(|arg| arg.get_id() == id),
                "`{path}` has no argument `{id}`"
            );
        }
    }
}
