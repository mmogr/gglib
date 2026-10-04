//! `--projector` and `--no-projector` against a real library and real GGUF
//! headers.

use std::path::PathBuf;

use gglib_core::services::ImportMode;

use super::*;
use crate::bootstrap::test_context;

/// A GGUF v3 file in `dir` holding only `pairs` as string metadata, under its
/// canonical path.
fn write_gguf(dir: &Path, name: &str, pairs: &[(&str, &str)]) -> PathBuf {
    let string = |text: &str| {
        let mut bytes = (text.len() as u64).to_le_bytes().to_vec();
        bytes.extend_from_slice(text.as_bytes());
        bytes
    };
    let mut bytes = b"GGUF".to_vec();
    bytes.extend_from_slice(&3_u32.to_le_bytes());
    bytes.extend_from_slice(&0_u64.to_le_bytes());
    bytes.extend_from_slice(&(pairs.len() as u64).to_le_bytes());
    for (key, value) in pairs {
        bytes.extend(string(key));
        bytes.extend_from_slice(&8_u32.to_le_bytes());
        bytes.extend(string(value));
    }
    let path = dir.join(name);
    std::fs::write(&path, bytes).unwrap();
    path.canonicalize().unwrap()
}

/// A library in `dir` holding one model, and that model.
async fn library(dir: &Path) -> (CliContext, Model) {
    let ctx = test_context(dir).await;
    let weights = write_gguf(dir, "qwen.Q8_0.gguf", &[("general.architecture", "qwen3")]);
    let model = ctx
        .app
        .models()
        .import_from_file(&weights, ctx.gguf_parser.as_ref(), None, ImportMode::Fresh)
        .await
        .expect("the model imports");
    (ctx, model)
}

async fn stored(ctx: &CliContext, id: i64) -> Model {
    ctx.app
        .models()
        .get_by_id(id)
        .await
        .unwrap()
        .expect("stored")
}

/// `--projector` links, and the update's own write that follows, made from
/// the row as it was read before the link, does not undo it.
#[tokio::test]
async fn a_projector_is_linked_and_survives_the_rest_of_the_update() {
    let dir = tempfile::tempdir().unwrap();
    let (ctx, model) = library(dir.path()).await;
    let projector = write_gguf(dir.path(), "mmproj-F16.gguf", &[("general.type", "mmproj")]);
    let mut updated = model.clone();
    updated.name = "Renamed".to_owned();

    apply(&ctx, &mut updated, Some(ProjectorChange::Link(&projector)))
        .await
        .unwrap();
    ctx.app.models().update(&updated).await.unwrap();

    let row = stored(&ctx, model.id).await;
    assert_eq!(row.projector_path.as_deref(), Some(projector.as_path()));
    assert!(row.image_input());
    assert_eq!(row.name, "Renamed");
}

#[tokio::test]
async fn no_projector_unlinks() {
    let dir = tempfile::tempdir().unwrap();
    let (ctx, model) = library(dir.path()).await;
    let projector = write_gguf(dir.path(), "mmproj-F16.gguf", &[("general.type", "mmproj")]);
    let mut linked = model.clone();
    apply(&ctx, &mut linked, Some(ProjectorChange::Link(&projector)))
        .await
        .unwrap();

    apply(&ctx, &mut linked, Some(ProjectorChange::Unlink))
        .await
        .unwrap();

    assert_eq!(linked.projector_path, None);
    assert!(!stored(&ctx, model.id).await.image_input());
}

/// Neither flag: the link the model has is carried through untouched.
#[tokio::test]
async fn neither_flag_leaves_the_link() {
    let dir = tempfile::tempdir().unwrap();
    let (ctx, model) = library(dir.path()).await;
    let projector = write_gguf(dir.path(), "mmproj-F16.gguf", &[("general.type", "mmproj")]);
    let mut linked = model.clone();
    apply(&ctx, &mut linked, Some(ProjectorChange::Link(&projector)))
        .await
        .unwrap();

    apply(&ctx, &mut linked, None).await.unwrap();
    ctx.app.models().update(&linked).await.unwrap();

    let row = stored(&ctx, model.id).await;
    assert_eq!(row.projector_path.as_deref(), Some(projector.as_path()));
}

/// The header decides, not the name: weights named like a projector are
/// refused by name and the model stays unlinked.
#[tokio::test]
async fn a_weights_file_is_refused_by_name() {
    let dir = tempfile::tempdir().unwrap();
    let (ctx, model) = library(dir.path()).await;
    let weights = write_gguf(
        dir.path(),
        "mmproj-fake.gguf",
        &[("general.architecture", "llama")],
    );
    let mut updated = model.clone();

    let refused = apply(&ctx, &mut updated, Some(ProjectorChange::Link(&weights)))
        .await
        .unwrap_err()
        .to_string();

    assert!(refused.contains("mmproj-fake.gguf"), "{refused}");
    assert!(refused.contains("not a projector"), "{refused}");
    assert_eq!(updated.projector_path, None);
    assert!(!stored(&ctx, model.id).await.image_input());
}

fn model_with(projector: Option<&str>) -> Model {
    let mut new = gglib_core::NewModel::new(
        "qwen".to_owned(),
        PathBuf::from("/models/qwen.gguf"),
        7.0,
        chrono::Utc::now(),
    );
    new.projector_path = projector.map(PathBuf::from);
    Model::stored(1, &new)
}

#[test]
fn the_preview_names_the_old_and_the_new_link() {
    let unlinked = model_with(None);
    let linked = model_with(Some("/models/mmproj-F16.gguf"));
    let path = Path::new("/models/mmproj-F16.gguf");

    assert_eq!(
        preview(&unlinked, Some(ProjectorChange::Link(path))).as_deref(),
        Some("  Projector:      -- → /models/mmproj-F16.gguf")
    );
    assert_eq!(
        preview(&linked, Some(ProjectorChange::Unlink)).as_deref(),
        Some("  Projector:      /models/mmproj-F16.gguf → --")
    );
}

#[test]
fn the_preview_is_silent_when_nothing_would_change() {
    let unlinked = model_with(None);
    let linked = model_with(Some("/models/mmproj-F16.gguf"));

    assert_eq!(preview(&linked, None), None);
    assert_eq!(preview(&unlinked, Some(ProjectorChange::Unlink)), None);
    assert_eq!(
        preview(
            &linked,
            Some(ProjectorChange::Link(Path::new("/models/mmproj-F16.gguf")))
        ),
        None
    );
}

/// `argv` parsed as the CLI parses it and run as `gglib model …` runs it.
async fn run(ctx: &CliContext, argv: &[&str]) -> Result<()> {
    use clap::Parser as _;
    let cli = crate::Cli::try_parse_from(argv)?;
    let Some(crate::Commands::Model { command }) = cli.command else {
        panic!("{argv:?} is not a model command");
    };
    super::super::dispatch(ctx, command, crate::target::Target::Local).await
}

/// From the command line to the stored row: the flags reach the handler, the
/// link is written, and a refused file fails the command and leaves the rest
/// of the same update unwritten.
#[tokio::test]
async fn the_command_links_unlinks_and_refuses() {
    let dir = tempfile::tempdir().unwrap();
    let (ctx, model) = library(dir.path()).await;
    let projector = write_gguf(dir.path(), "mmproj-F16.gguf", &[("general.type", "mmproj")]);
    let id = model.id.to_string();
    let update = ["gglib", "model", "update", &id, "--force"];

    let link = [&update[..], &["--projector", projector.to_str().unwrap()]].concat();
    run(&ctx, &link).await.unwrap();
    let row = stored(&ctx, model.id).await;
    assert_eq!(row.projector_path.as_deref(), Some(projector.as_path()));

    let weights = model.file_path.to_str().unwrap();
    let refuse = [&update[..], &["--name", "Renamed", "--projector", weights]].concat();
    let refused = run(&ctx, &refuse).await.unwrap_err().to_string();
    assert!(refused.contains("not a projector"), "{refused}");
    let row = stored(&ctx, model.id).await;
    assert_eq!(row.name, model.name, "a refused update wrote its name");
    assert_eq!(row.projector_path.as_deref(), Some(projector.as_path()));

    run(&ctx, &[&update[..], &["--no-projector"]].concat())
        .await
        .unwrap();
    assert!(!stored(&ctx, model.id).await.image_input());
}
