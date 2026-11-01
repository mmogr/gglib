# ADR 0028 — `--remote` is declared on each command that reaches the paired machine

- **Status:** Accepted
- **Date:** 2026-11-01
- **Depends on:** [ADR 0012](0012-the-remote-tunnel.md),
  [ADR 0013](0013-the-target-is-a-value.md)
- **Supersedes:** the part of [ADR 0013](0013-the-target-is-a-value.md) that
  declared `--remote` once, as a global argument on the root parser
  (decision 1), and decision 3's hand-written list of the commands it
  reaches; nothing else in it
- **Superseded by:** nothing
- **Log:** none

## Context

ADR 0013 made `--remote` a global argument, so that "the parser does not
know which commands it means anything to". clap copies a global argument,
and its help line, into every subcommand. So `gglib image --help` offered
`--remote`, described as "Do this on the machine this one is paired with",
and `gglib image -m 11 "a happy kitty" --remote` then refused it
([#1707](https://github.com/mmogr/gglib/issues/1707)). The same help line
was on every command that refuses the flag: `config`, `mcp`, `benchmark`,
`model add`, and the rest of ADR 0013's this-machine side.

Hiding the global flag on the commands that refuse it is not available in
clap 4.6.7, the version this workspace locks. It was tried on a model of
gglib's parser (one global `--remote`, a `chat` that takes it, an `image`
and a `model add` that refuse it):

- `Cli::command().mut_subcommand("image", |s| s.mut_arg("remote", |a|
  a.hide(true)))` before the command is built panics with
  ``Argument `remote` is undefined``: the global argument is copied into
  subcommands only by `Command::build()`.
- The same mutation after `cmd.build()` leaves the subcommand broken:
  `image --help` stopped printing help and reported the missing prompt,
  and `image x --remote` printed help instead of parsing.

Declaring the flag on each command that takes it, as a flattened
`#[derive(Args)]` struct, worked on the same model: `chat --help` showed
`--remote`, `image --help` and `model add --help` did not, and `chat
--remote` parsed.

The refusal's list of what the flag reaches was a hand-written constant
(`REACHES` in `crates/gglib-cli/src/target.rs`), kept by hand "so that the
sentence a person reads is written by a person and stays in the order they
would say it". It had already drifted from ADR 0013 decision 3, which
lists seven commands; the constant named an eighth, `model inspect`.

## Decision

### 1. The flag is declared on each command that reaches the paired machine

`RemoteFlag` (`crates/gglib-cli/src/remote_flag.rs`) is one
`#[derive(Args)]` struct with one `--remote` switch, flattened into each
command that uses the paired machine, and nowhere else. `--help` and the
generated shell completions show it exactly where it works. ADR 0013's line
still decides which commands those are: **use the machine, don't change
it**, and the `model` commands still take their answer from core's
`ModelAction::on_paired()`, which the web page reads too.

### 2. A command that does not take it still refuses it with a sentence

ADR 0013 decision 3's refusal stands: a command that is about this machine
says so in one sentence, naming what `--remote` does reach, and never
ignores the flag. clap rejects `--remote` on such a command as an unknown
argument; the CLI then parses the command line again without it and prints
ADR 0013's sentence, or the command's own (`proxy stop`, `web`, `gui`). The
flag now goes after the command it belongs to: `gglib --remote chat` is
answered with the command line to type instead.

### 3. The list in the refusal is read from the parser

The commands `--remote` reaches, as the refusal names them, are the
subcommands that declare the flag, named and ordered as `gglib --help`
names and orders them: `question`, not its alias `q`. This gives up the
hand-written sentence `target.rs` chose, because a hand-written list
drifts and this one had. A test holds the parser to ADR 0013's table: each
command that takes the flag is one `target::reach` marks as using a
machine, and each `model` subcommand takes it exactly when
`ModelAction::on_paired()` allows its action there.

### 4. Every route the tunnel carries is reached or exempted

`crates/gglib-cli/src/target_tunnel_tests.rs` maps each route in
`gglib-proxy`'s `TUNNEL_REACHABLE` to the command lines that use it, or to
the reason none does. It fails when the tunnel carries a route the table
does not name, when it names a command that refuses `--remote`, and when a
command that takes `--remote` uses no route in it. The table is written by
hand: nothing links a route to a command mechanically, so the test makes
the judgement visible rather than making it.

## Consequences

- A person reads `--remote` only on a command that will act on it.
- `gglib --remote <command>` no longer parses. Nothing in the repository
  wrote the flag before the command except the parser's own test.
- Adding the flag to a command is one field, and the tunnel table then asks
  which route it uses.

## Out of scope

Which commands should reach the paired machine. This ADR changes where the
flag is declared and how its list is kept, not ADR 0013's line; a command
moved across that line, such as `gglib image` for #1707, is its own change.
