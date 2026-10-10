//! `gglib image`'s arguments, its live line, where it saves, and what a
//! failure says.

use std::path::{Path, PathBuf};
use std::time::Duration;

use clap::Parser as _;
use gglib_core::ports::{ImageError, ImageProgress, ImageSize, ImageStage};

use super::{INTERRUPTED, PlainLines, failure, line, output_paths, say};
use crate::commands::Commands;
use crate::image_commands::ImageCommandArgs;
use crate::parser::Cli;
use crate::target::{Reach, reach};

fn parsed(argv: &[&str]) -> ImageCommandArgs {
    match Cli::parse_from(argv).command {
        Some(Commands::Image(image)) => image,
        _ => panic!("expected gglib image"),
    }
}

#[test]
fn image_takes_a_prompt_size_count_seed_model_and_output() {
    let args = parsed(&[
        "gglib",
        "image",
        "a red fox",
        "--size",
        "512x768",
        "-n",
        "2",
        "--seed",
        "3",
        "-m",
        "flux",
        "-o",
        "fox.png",
    ]);
    assert_eq!(args.prompt, "a red fox");
    assert_eq!(
        args.size,
        Some(ImageSize {
            width: 512,
            height: 768
        })
    );
    assert_eq!((args.n, args.seed), (2, Some(3)));
    assert_eq!(args.model.as_deref(), Some("flux"));
    assert_eq!(args.output, Some(PathBuf::from("fox.png")));

    let plain = parsed(&["gglib", "image", "a red fox"]);
    assert_eq!((plain.n, plain.size, plain.model), (1, None, None));
}

#[test]
fn image_refuses_a_count_or_size_it_cannot_ask_for() {
    for argv in [
        &["gglib", "image", "a fox", "-n", "5"][..],
        &["gglib", "image", "a fox", "-n", "0"],
        &["gglib", "image", "a fox", "--size", "big"],
        &["gglib", "image"],
    ] {
        assert!(Cli::try_parse_from(argv).is_err(), "{argv:?}");
    }
}

/// Drawing uses this machine's daemon; `--remote` does not reach it.
#[test]
fn image_is_this_machines() {
    let command = Cli::parse_from(["gglib", "image", "a fox"])
        .command
        .unwrap();
    assert!(matches!(reach(&command), ("image", Reach::Local)));
}

fn report(stage: ImageStage) -> ImageProgress {
    ImageProgress::stage(stage)
}

#[test]
fn the_live_line_reads_each_stage() {
    assert_eq!(
        line(
            &report(ImageStage::Queued {
                position: 2,
                behind: None
            }),
            1
        ),
        "queued, place 2"
    );
    assert_eq!(
        line(
            &report(ImageStage::Queued {
                position: 1,
                behind: Some("an image render".into())
            }),
            1
        ),
        "queued behind an image render, place 1"
    );
    assert_eq!(line(&report(ImageStage::Loading), 1), "loading the model…");
    let step = report(ImageStage::Sampling {
        pass: 2,
        step: 3,
        total: 4,
    });
    assert_eq!(line(&step, 1), "sampling 3/4");
    assert_eq!(line(&step, 2), "sampling 3/4 (image 2 of 2)");
    assert_eq!(line(&report(ImageStage::Decoding), 1), "decoding");
}

/// With no terminal to redraw a line on, a render prints each stage once
/// and each sampling step once: a report that repeats the line before it
/// prints nothing.
#[test]
fn without_a_terminal_each_stage_and_step_is_one_plain_line() {
    let sampling = |step| {
        report(ImageStage::Sampling {
            pass: 1,
            step,
            total: 4,
        })
    };
    let reports = [
        report(ImageStage::Loading),
        report(ImageStage::Loading),
        sampling(1),
        sampling(1),
        sampling(2),
        report(ImageStage::Decoding),
        report(ImageStage::Decoding),
    ];
    let mut plain = PlainLines::default();
    let printed: Vec<String> = reports
        .iter()
        .filter_map(|report| plain.changed(line(report, 1)))
        .collect();
    assert_eq!(
        printed,
        [
            "loading the model…",
            "sampling 1/4",
            "sampling 2/4",
            "decoding"
        ]
    );
}

/// Where the console draws bars the line goes on the bar and nothing is
/// printed; anywhere else the bar is left alone and the line is printed.
#[test]
fn a_line_goes_on_the_bar_where_one_is_drawn_and_is_printed_plain_elsewhere() {
    let bar = indicatif::ProgressBar::hidden();
    let mut plain = PlainLines::default();

    let printed = say(true, &bar, &mut plain, "loading the model…".to_owned());
    assert_eq!(printed, None);
    assert_eq!(bar.message(), "loading the model…");

    let printed = say(false, &bar, &mut plain, "sampling 1/4".to_owned());
    assert_eq!(printed.as_deref(), Some("sampling 1/4"));
    assert_eq!(bar.message(), "loading the model…");
    let again = say(false, &bar, &mut plain, "sampling 1/4".to_owned());
    assert_eq!(again, None, "a line is printed once");
}

#[test]
fn images_are_saved_where_asked_or_by_time_here() {
    assert_eq!(
        output_paths(None, 1, 1_700),
        [PathBuf::from("gglib-1700.png")]
    );
    assert_eq!(
        output_paths(None, 2, 1_700),
        [
            PathBuf::from("gglib-1700-1.png"),
            PathBuf::from("gglib-1700-2.png")
        ]
    );
    assert_eq!(
        output_paths(Some(Path::new("out/fox.png")), 1, 1_700),
        [PathBuf::from("out/fox.png")]
    );
    assert_eq!(
        output_paths(Some(Path::new("out/fox.png")), 3, 1_700),
        [
            PathBuf::from("out/fox-1.png"),
            PathBuf::from("out/fox-2.png"),
            PathBuf::from("out/fox-3.png")
        ]
    );
    assert_eq!(
        output_paths(Some(Path::new("fox")), 2, 1_700),
        [PathBuf::from("fox-1.png"), PathBuf::from("fox-2.png")]
    );
}

/// A failure is the daemon's words: "cannot draw" where nothing here can,
/// "the render failed" where it tried; an unreachable daemon says only that.
#[test]
fn a_failure_says_the_daemons_words() {
    let refused = |code: &str, message: &str| ImageError::Refused {
        status: 400,
        code: Some(code.into()),
        message: message.into(),
    };
    assert_eq!(
        failure(&refused("drawing_unavailable", "there is no image model")),
        "cannot draw: there is no image model"
    );
    assert_eq!(
        failure(&refused(
            "image_render_stalled",
            "the render made no progress"
        )),
        "the render failed: the render made no progress"
    );
    let unreachable = ImageError::Refused {
        status: 0,
        code: None,
        message: "could not reach the gglib daemon: refused".into(),
    };
    assert_eq!(
        failure(&unreachable),
        "could not reach the gglib daemon: refused"
    );
    assert!(
        failure(&ImageError::Stalled {
            after: Duration::from_mins(3)
        })
        .starts_with("the render failed:")
    );
    assert!(INTERRUPTED.contains("cannot be interrupted"));
}
