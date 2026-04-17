//! Rustyline-based interactive editing REPL for council configuration.
//!
//! Commands:
//!   show                — re-display the council summary table
//!   persona <N>         — edit agent N's persona (single-line prompt)
//!   cont <N>            — edit agent N's contentiousness
//!   tools <N>           — edit agent N's tool filter (prints list first)
//!   rounds <N>          — set the number of deliberation rounds
//!   remove <N>          — remove agent N
//!   run                 — accept and run the council
//!   save <path>         — write the config to a JSON file
//!   quit / q            — abort without running

use std::io::Write as _;

use anyhow::Result;
use rustyline::DefaultEditor;

use gglib_agent::council::config::CouncilConfig;

use crate::presentation::style::{BOLD, DIM, RESET};

use super::editor;
use super::render;

/// Run the interactive editing REPL.  Returns `Some(config)` if the user
/// chose `run`, or `None` if they quit.
pub fn edit_loop(config: &mut CouncilConfig, available_tools: &[String]) -> Result<Option<()>> {
    let mut rl = DefaultEditor::new()?;

    print_help();

    loop {
        let line = match rl.readline(&format!("{BOLD}council>{RESET} ")) {
            Ok(l) => l,
            Err(
                rustyline::error::ReadlineError::Interrupted | rustyline::error::ReadlineError::Eof,
            ) => {
                eprintln!("{DIM}Aborted.{RESET}");
                return Ok(None);
            }
            Err(e) => return Err(e.into()),
        };
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let _ = rl.add_history_entry(line);

        let (cmd, arg) = split_command(line);
        match cmd {
            "help" | "h" | "?" => print_help(),
            "show" | "s" => render::render_config(config),
            "run" | "r" => return Ok(Some(())),
            "quit" | "q" => {
                eprintln!("{DIM}Aborted.{RESET}");
                return Ok(None);
            }
            "rounds" => match arg {
                Some(a) => report(editor::apply_rounds(config, a)),
                None => eprintln!("  usage: rounds <number>"),
            },
            "remove" => match arg {
                Some(a) => {
                    report(editor::remove_agent(config, a));
                    render::render_config(config);
                }
                None => eprintln!("  usage: remove <agent#>"),
            },
            "persona" => {
                if let Some(idx) = parse_agent_idx(arg, config.agents.len()) {
                    eprint!("  New persona for {}: ", config.agents[idx].name);
                    let _ = std::io::stderr().flush();
                    let input = rl.readline("  ")?;
                    let res = editor::apply_persona(&mut config.agents[idx], input.trim());
                    report(res);
                    editor::print_agent_summary(idx, &config.agents[idx]);
                }
            }
            "cont" => {
                if let Some(idx) = parse_agent_idx(arg, config.agents.len()) {
                    eprint!(
                        "  New contentiousness for {} (0.0–1.0): ",
                        config.agents[idx].name
                    );
                    let _ = std::io::stderr().flush();
                    let input = rl.readline("  ")?;
                    let res = editor::apply_contentiousness(&mut config.agents[idx], input.trim());
                    report(res);
                    editor::print_agent_summary(idx, &config.agents[idx]);
                }
            }
            "tools" => {
                if let Some(idx) = parse_agent_idx(arg, config.agents.len()) {
                    editor::print_available_tools(available_tools);
                    eprintln!("  Enter comma-separated tool names, or \"all\":");
                    let input = rl.readline("  ")?;
                    let res = editor::apply_tool_filter(
                        &mut config.agents[idx],
                        input.trim(),
                        available_tools,
                    );
                    report(res);
                    editor::print_agent_summary(idx, &config.agents[idx]);
                }
            }
            "save" => match arg {
                Some(path) => {
                    let json = serde_json::to_string_pretty(config)?;
                    std::fs::write(path, &json)?;
                    eprintln!("  {DIM}Saved to {path}{RESET}");
                }
                None => eprintln!("  usage: save <path.json>"),
            },
            other => eprintln!("  unknown command: {other}  (type \"help\" for commands)"),
        }
    }
}

// ─── Helpers ────────────────────────────────────────────────────────────────

fn split_command(line: &str) -> (&str, Option<&str>) {
    match line.split_once(char::is_whitespace) {
        Some((cmd, rest)) => (cmd, Some(rest.trim())),
        None => (line, None),
    }
}

fn parse_agent_idx(arg: Option<&str>, count: usize) -> Option<usize> {
    let Some(s) = arg else {
        eprintln!("  expected an agent number (1–{count})");
        return None;
    };
    match s.parse::<usize>() {
        Ok(n) if n >= 1 && n <= count => Some(n - 1),
        _ => {
            eprintln!("  agent number must be between 1 and {count}");
            None
        }
    }
}

fn report(result: Result<()>) {
    if let Err(e) = result {
        eprintln!("  \x1b[31m{e}{RESET}");
    }
}

fn print_help() {
    eprintln!(
        "\n{BOLD}Council Editor Commands:{RESET}
  show             re-display the council summary
  persona <N>      edit agent N's persona
  cont <N>         edit agent N's contentiousness
  tools <N>        edit agent N's tool filter
  rounds <N>       set number of rounds
  remove <N>       remove agent N
  save <path>      save config to JSON file
  run              accept and run the council
  quit             abort without running
"
    );
}
