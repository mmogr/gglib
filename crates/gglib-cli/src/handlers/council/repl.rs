//! Rustyline-based interactive editing REPL for council configuration.
//!
//! Commands:
//!   show                — re-display the council summary table
//!   name <N>            — rename agent N (offers AI-fill afterwards)
//!   persona <N>         — edit agent N's persona (single-line prompt)
//!   cont <N>            — edit agent N's contentiousness
//!   tools <N>           — edit agent N's tool filter (prints list first)
//!   fill <N>            — ask the LLM to fill agent N's details
//!   rounds <N>          — set the number of deliberation rounds
//!   add                 — add a new agent (prompts for name, offers fill)
//!   remove <N>          — remove agent N
//!   refine <msg>        — ask the LLM to revise the council
//!   run                 — accept and run the council
//!   save <path>         — write the config to a JSON file
//!   quit / q            — abort without running

use std::io::Write as _;

use anyhow::Result;
use rustyline::DefaultEditor;

use gglib_agent::council::config::{CouncilAgent, CouncilConfig};

use crate::presentation::style::{BOLD, DIM, RESET};

use super::editor;
use super::render;

/// Outcome of a single REPL session.
pub enum EditOutcome {
    /// User chose `run` — proceed with deliberation.
    Run,
    /// User chose `quit` — abort without running.
    Quit,
    /// User chose `refine <msg>` — caller should re-suggest then re-enter REPL.
    Refine(String),
    /// User wants the LLM to fill an agent's details (by 0-based index).
    Fill(usize),
}

/// Run the interactive editing REPL.
pub fn edit_loop(config: &mut CouncilConfig, available_tools: &[String]) -> Result<EditOutcome> {
    let mut rl = DefaultEditor::new()?;

    print_help();

    loop {
        let line = match rl.readline(&format!("{BOLD}council>{RESET} ")) {
            Ok(l) => l,
            Err(
                rustyline::error::ReadlineError::Interrupted | rustyline::error::ReadlineError::Eof,
            ) => {
                eprintln!("{DIM}Aborted.{RESET}");
                return Ok(EditOutcome::Quit);
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
            "run" | "r" => return Ok(EditOutcome::Run),
            "quit" | "q" => {
                eprintln!("{DIM}Aborted.{RESET}");
                return Ok(EditOutcome::Quit);
            }
            "refine" => match arg {
                Some(instruction) => return Ok(EditOutcome::Refine(instruction.to_owned())),
                None => eprintln!("  usage: refine <instruction>"),
            },
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
            "name" => {
                if let Some(idx) = parse_agent_idx(arg, config.agents.len()) {
                    eprint!("  New name for #{} ({}): ", idx + 1, config.agents[idx].name);
                    let _ = std::io::stderr().flush();
                    let input = rl.readline("  ")?;
                    let res = editor::apply_name(&mut config.agents[idx], input.trim());
                    report(res);
                    editor::print_agent_summary(idx, &config.agents[idx]);
                    if offer_fill(&mut rl, &config.agents[idx].name)? {
                        return Ok(EditOutcome::Fill(idx));
                    }
                }
            }
            "fill" => {
                if let Some(idx) = parse_agent_idx(arg, config.agents.len()) {
                    return Ok(EditOutcome::Fill(idx));
                }
            }
            "add" => {
                let eprint_msg = format!(
                    "  Name for the new agent (#{}): ",
                    config.agents.len() + 1
                );
                eprint!("{eprint_msg}");
                let _ = std::io::stderr().flush();
                let input = rl.readline("  ")?;
                let name = input.trim();
                if name.is_empty() {
                    eprintln!("  name cannot be empty");
                } else {
                    let idx = config.agents.len();
                    config.agents.push(scaffold_agent(name, idx));
                    render::render_config(config);
                    if offer_fill(&mut rl, name)? {
                        return Ok(EditOutcome::Fill(idx));
                    }
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

/// Prompt the user to AI-fill details for a named agent. Returns `true` on "y".
fn offer_fill(rl: &mut DefaultEditor, agent_name: &str) -> Result<bool> {
    eprintln!("  Let the LLM fill in details for '{agent_name}'? [y/N] ");
    let answer = rl.readline("  ")?;
    Ok(matches!(answer.trim(), "y" | "Y" | "yes"))
}

/// Create a scaffold agent with a slugified ID and cycled color.
fn scaffold_agent(name: &str, idx: usize) -> CouncilAgent {
    let slug: String = name
        .chars()
        .map(|c| if c.is_alphanumeric() { c.to_ascii_lowercase() } else { '-' })
        .collect::<String>()
        .trim_matches('-')
        .to_owned();
    let id = if slug.is_empty() {
        format!("agent-{idx}")
    } else {
        format!("{slug}-{idx}")
    };
    let colors = [
        "#3b82f6", "#ef4444", "#10b981", "#f59e0b",
        "#8b5cf6", "#ec4899", "#06b6d4", "#f97316",
    ];
    CouncilAgent {
        id,
        name: name.to_owned(),
        color: colors[idx % colors.len()].to_owned(),
        persona: String::from("Define this agent's worldview and expertise."),
        perspective: String::from("Describe their unique angle."),
        contentiousness: 0.5,
        tool_filter: None,
    }
}

fn print_help() {
    eprintln!(
        "\n{BOLD}Council Editor Commands:{RESET}
  show             re-display the council summary
  name <N>         rename agent N (offers AI-fill)
  persona <N>      edit agent N's persona
  cont <N>         edit agent N's contentiousness
  tools <N>        edit agent N's tool filter
  fill <N>         ask the LLM to fill agent N's details
  rounds <N>       set number of rounds
  add              add a new agent
  remove <N>       remove agent N
  refine <msg>     ask the LLM to revise the council
  save <path>      save config to JSON file
  run              accept and run the council
  quit             abort without running
"
    );
}
