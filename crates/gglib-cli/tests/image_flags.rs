//! `--image <PATH>` on `gglib q` and `gglib chat`, as clap parses it:
//! repeatable, in order, long form only, and never in place of the question.

use std::path::PathBuf;

use clap::Parser;
use clap::error::ErrorKind;
use gglib_cli::{Cli, Commands};

#[path = "support/flag_surface.rs"]
mod flag_surface;

/// The image paths `argv` parses to, on whichever of the two commands it
/// names.
fn images(argv: &[&str]) -> Result<Vec<PathBuf>, clap::Error> {
    match Cli::try_parse_from(argv)?.command {
        Some(Commands::Question { images, .. } | Commands::Chat { images, .. }) => {
            Ok(images.images)
        }
        _ => panic!("{argv:?} did not parse as `q` or `chat`"),
    }
}

fn paths(names: &[&str]) -> Vec<PathBuf> {
    names.iter().map(PathBuf::from).collect()
}

#[test]
fn q_takes_one_image_per_flag_in_the_order_given() {
    let argv = [
        "gglib", "q", "--image", "b.png", "what?", "--image", "a.jpg",
    ];

    assert_eq!(images(&argv).unwrap(), paths(&["b.png", "a.jpg"]));
}

#[test]
fn chat_takes_one_image_per_flag_in_the_order_given() {
    let argv = [
        "gglib", "chat", "qwen", "--image", "b.png", "--image", "a.jpg",
    ];

    assert_eq!(images(&argv).unwrap(), paths(&["b.png", "a.jpg"]));
}

#[test]
fn no_flag_attaches_nothing() {
    assert_eq!(images(&["gglib", "q", "what?"]).unwrap(), paths(&[]));
    assert_eq!(images(&["gglib", "chat", "qwen"]).unwrap(), paths(&[]));
}

/// One flag names one file: a second path after it is the question, not a
/// second image.
#[test]
fn one_flag_takes_one_path() {
    let argv = ["gglib", "q", "--image", "a.png", "b.png"];

    assert_eq!(images(&argv).unwrap(), paths(&["a.png"]));
}

#[test]
fn q_with_an_image_still_requires_a_question() {
    let refused = images(&["gglib", "q", "--image", "a.png"]).unwrap_err();

    assert_eq!(refused.kind(), ErrorKind::MissingRequiredArgument);
}

#[test]
fn the_flag_without_a_path_is_refused() {
    for command in [
        &["gglib", "q", "what?", "--image"][..],
        &["gglib", "chat", "qwen", "--image"],
    ] {
        let refused = images(command).unwrap_err();
        assert_eq!(refused.kind(), ErrorKind::InvalidValue, "{command:?}");
    }
}

/// Both commands carry the flag, and neither gives it a short form: every
/// letter on `q` and `chat` already means something.
#[test]
fn both_commands_expose_the_flag_in_its_long_form_only() {
    use clap::CommandFactory as _;
    let root = Cli::command();

    for name in ["question", "chat"] {
        assert!(
            flag_surface::long_flags(name).contains(&"image".to_owned()),
            "`{name}` is missing --image"
        );
        let command = root.find_subcommand(name).expect("the subcommand");
        let image = command
            .get_arguments()
            .find(|arg| arg.get_long() == Some("image"))
            .expect("--image");
        assert_eq!(image.get_short(), None, "`{name}`");
    }
}

/// The two flags that moved into a shared group with this change still
/// land on both commands.
#[test]
fn both_commands_keep_the_tool_limit_flags() {
    for name in ["question", "chat"] {
        let flags = flag_surface::long_flags(name);
        for flag in ["tool-timeout-ms", "max-parallel"] {
            assert!(
                flags.contains(&flag.to_owned()),
                "`{name}` is missing --{flag}"
            );
        }
    }
}
