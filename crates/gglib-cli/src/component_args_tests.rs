//! `--component` and `--no-component` as clap parses them and as the
//! request carries them.

use clap::Parser as _;
use clap::error::ErrorKind;

use super::*;

/// The component flags `argv` parses to on `gglib model update 3`.
fn parsed(flags: &[&str]) -> Result<ComponentArgs, clap::Error> {
    let argv = ["gglib", "model", "update", "3"].iter().chain(flags);
    let cli = crate::Cli::try_parse_from(argv)?;
    let Some(crate::Commands::Model {
        command: crate::ModelCommand::Update { components, .. },
    }) = cli.command
    else {
        panic!("`model update` did not parse as a model update");
    };
    Ok(components)
}

/// `role=path`, repeated, lands as each role and its path; `--no-component`
/// as each role to unlink.
#[test]
fn each_role_lands_with_its_path_or_as_an_unlink() {
    let args = parsed(&[
        "--component",
        "vae=/m/ae.safetensors",
        "--component",
        "t5xxl=/m/t5xxl_fp16.safetensors",
        "--no-component",
        "clip_l",
    ])
    .unwrap();

    let changes = args.changes().unwrap();
    assert_eq!(
        changes.into_iter().collect::<Vec<_>>(),
        [
            (ComponentRole::Vae, Some(Path::new("/m/ae.safetensors"))),
            (ComponentRole::ClipL, None),
            (
                ComponentRole::T5xxl,
                Some(Path::new("/m/t5xxl_fp16.safetensors"))
            ),
        ]
    );
}

/// A path holding `=` keeps it: only the first one splits.
#[test]
fn only_the_first_equals_sign_splits() {
    let args = parsed(&["--component", "llm=/m/a=b.gguf"]).unwrap();

    assert_eq!(
        args.component,
        [(ComponentRole::Llm, PathBuf::from("/m/a=b.gguf"))]
    );
}

/// A role that is none of the four is refused by name, with the names there
/// are, in either flag.
#[test]
fn an_unknown_role_is_refused_by_name() {
    for flags in [
        &["--component", "unet=/m/u.safetensors"][..],
        &["--no-component", "unet"],
    ] {
        let refused = parsed(flags).unwrap_err();

        assert_eq!(refused.kind(), ErrorKind::ValueValidation, "{flags:?}");
        let said = refused.to_string();
        assert!(said.contains("unknown component role \"unet\""), "{said}");
        assert!(said.contains("vae, clip_l, t5xxl, llm"), "{said}");
    }
}

/// A value with no `=`, or nothing after it, is refused before anything is
/// asked of the library.
#[test]
fn a_value_without_a_path_is_refused() {
    let refused = parsed(&["--component", "vae"]).unwrap_err().to_string();
    assert!(refused.contains("expected <ROLE>=<PATH>"), "{refused}");

    let refused = parsed(&["--component", "vae="]).unwrap_err().to_string();
    assert!(refused.contains("no path after vae="), "{refused}");
}

/// One role named twice says two things about one link, whichever flags
/// name it.
#[test]
fn a_role_named_twice_is_refused() {
    for flags in [
        &["--component", "vae=/a", "--no-component", "vae"][..],
        &["--component", "vae=/a", "--component", "vae=/b"],
        &["--no-component", "vae", "--no-component", "vae"],
    ] {
        let refused = parsed(flags).unwrap().changes().unwrap_err().to_string();
        assert!(
            refused.contains("vae is named twice"),
            "{flags:?}: {refused}"
        );
    }
}

/// No flag changes no component: the request carries no field at all, so
/// every link is left as it is.
#[test]
fn no_flag_asks_for_nothing() {
    let args = parsed(&[]).unwrap();

    assert!(args.changes().unwrap().is_empty());
    assert_eq!(request_components(&args.changes().unwrap()), None);
}

/// The request names a link by its path and an unlink as `null`.
#[test]
fn the_request_carries_paths_and_unlinks() {
    let args = parsed(&[
        "--component",
        "vae=/m/ae.safetensors",
        "--no-component",
        "llm",
    ])
    .unwrap();

    let request = request_components(&args.changes().unwrap()).unwrap();

    assert_eq!(
        request.into_iter().collect::<Vec<_>>(),
        [
            (ComponentRole::Vae, Some("/m/ae.safetensors".to_owned())),
            (ComponentRole::Llm, None),
        ]
    );
}

/// `model add` takes the same flags.
#[test]
fn model_add_takes_the_same_flags() {
    let cli = crate::Cli::try_parse_from([
        "gglib",
        "model",
        "add",
        "/m/flux.gguf",
        "--component",
        "vae=/m/ae.safetensors",
        "--no-component",
        "clip_l",
    ])
    .unwrap();
    let Some(crate::Commands::Model {
        command: crate::ModelCommand::Add { components, .. },
    }) = cli.command
    else {
        panic!("`model add` did not parse as an add");
    };

    assert_eq!(
        components.component,
        [(ComponentRole::Vae, PathBuf::from("/m/ae.safetensors"))]
    );
    assert_eq!(components.no_component, [ComponentRole::ClipL]);
}
