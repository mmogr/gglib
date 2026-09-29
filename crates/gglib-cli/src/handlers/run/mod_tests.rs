//! `gglib run`'s arguments, and what it refuses before a request.

use clap::Parser as _;

use super::{RunCommand, checked};
use crate::commands::Commands;
use crate::parser::Cli;
use crate::target::{Reach, Target, reach};

fn subcommand(argv: &[&str]) -> RunCommand {
    match Cli::parse_from(argv).command {
        Some(Commands::Run(parsed)) => parsed.command,
        _ => panic!("expected gglib run"),
    }
}

#[test]
fn start_takes_a_model_a_prompt_and_follow() {
    let RunCommand::Start {
        model,
        prompt,
        follow,
    } = subcommand(&[
        "gglib",
        "run",
        "start",
        "--model",
        "qwen",
        "why is the sky blue",
        "--follow",
    ])
    else {
        panic!("expected start");
    };
    assert_eq!(
        (model.as_str(), prompt.as_str(), follow),
        ("qwen", "why is the sky blue", true)
    );
    assert!(
        Cli::try_parse_from(["gglib", "run", "start", "hi"]).is_err(),
        "--model is required"
    );
}

#[test]
fn list_show_and_cancel_parse() {
    assert!(matches!(
        subcommand(&["gglib", "run", "list"]),
        RunCommand::List
    ));
    assert!(matches!(
        subcommand(&["gglib", "run", "show", "run-1", "-f"]),
        RunCommand::Show { id, follow: true } if id == "run-1"
    ));
    assert!(matches!(
        subcommand(&["gglib", "run", "cancel", "run-1"]),
        RunCommand::Cancel { id } if id == "run-1"
    ));
}

#[test]
fn run_is_about_this_machine() {
    let command = Cli::parse_from(["gglib", "run", "list"]).command.unwrap();
    assert_eq!(reach(&command), ("run", Reach::Local));
    assert!(Target::from_flag(true).admit(&command).is_err());
}

#[test]
fn an_id_that_could_name_another_route_is_refused_before_any_request() {
    assert!(checked("run-0a1b2c3d4e5f").is_ok());
    for bad in ["../../models/7", "a/b", "", "a b"] {
        assert!(checked(bad).is_err(), "{bad:?}");
    }
}
