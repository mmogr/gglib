//! `gglib model update --projector <path>` and `--no-projector`, as clap
//! parses them: each lands on its field, and the two together are refused.

use std::path::Path;

use clap::Parser;
use clap::error::ErrorKind;
use gglib_cli::{Cli, Commands, ModelCommand};

/// The projector path and the unlink switch `argv` parses to.
fn parsed(flags: &[&str]) -> Result<(Option<std::path::PathBuf>, bool), clap::Error> {
    let argv = ["gglib", "model", "update", "3"].iter().chain(flags);
    let cli = Cli::try_parse_from(argv)?;
    let Some(Commands::Model {
        command: ModelCommand::Update { projector, .. },
    }) = cli.command
    else {
        panic!("`model update` did not parse as a model update");
    };
    Ok((projector.projector, projector.no_projector))
}

#[test]
fn projector_takes_a_path() {
    let (path, unlink) = parsed(&["--projector", "/models/mmproj-F16.gguf"]).unwrap();

    assert_eq!(path.as_deref(), Some(Path::new("/models/mmproj-F16.gguf")));
    assert!(!unlink);
}

#[test]
fn no_projector_is_a_switch() {
    let (path, unlink) = parsed(&["--no-projector"]).unwrap();

    assert_eq!(path, None);
    assert!(unlink);
}

#[test]
fn neither_flag_asks_for_no_change() {
    assert_eq!(parsed(&[]).unwrap(), (None, false));
}

/// Linking and unlinking in one command says two things about one link, in
/// either order.
#[test]
fn the_two_flags_together_are_refused() {
    for flags in [
        ["--projector", "/models/mmproj-F16.gguf", "--no-projector"],
        ["--no-projector", "--projector", "/models/mmproj-F16.gguf"],
    ] {
        let refused = parsed(&flags).unwrap_err();
        assert_eq!(refused.kind(), ErrorKind::ArgumentConflict, "{flags:?}");
    }
}

#[test]
fn projector_without_a_path_is_refused() {
    let refused = parsed(&["--projector"]).unwrap_err();

    assert_eq!(refused.kind(), ErrorKind::InvalidValue);
}
