//! `--projector` and `--no-projector` against a real library and real GGUF
//! headers, from the command line to the stored row.

use std::path::PathBuf;

use super::super::test_library::{library, run, stored, write_gguf};
use super::*;

/// `--projector` links, and the rest of the same update is written beside
/// the link, not over it.
#[tokio::test]
async fn a_projector_is_linked_and_survives_the_rest_of_the_update() {
    let dir = tempfile::tempdir().unwrap();
    let (ctx, model) = library(dir.path()).await;
    let projector = write_gguf(dir.path(), "mmproj-F16.gguf", &[("general.type", "mmproj")]);
    let id = model.id.to_string();

    run(
        &ctx,
        &[
            "gglib",
            "model",
            "update",
            &id,
            "--force",
            "--name",
            "Renamed",
            "--projector",
            projector.to_str().unwrap(),
        ],
    )
    .await
    .unwrap();

    let row = stored(&ctx, model.id).await;
    assert_eq!(row.projector_path.as_deref(), Some(projector.as_path()));
    assert!(row.image_input());
    assert_eq!(row.name, "Renamed");
}

/// Neither flag: the link the model has is carried through an update of
/// something else untouched.
#[tokio::test]
async fn neither_flag_leaves_the_link() {
    let dir = tempfile::tempdir().unwrap();
    let (ctx, model) = library(dir.path()).await;
    let projector = write_gguf(dir.path(), "mmproj-F16.gguf", &[("general.type", "mmproj")]);
    let id = model.id.to_string();
    let update = ["gglib", "model", "update", &id, "--force"];
    let link = [&update[..], &["--projector", projector.to_str().unwrap()]].concat();
    run(&ctx, &link).await.unwrap();

    run(&ctx, &[&update[..], &["--name", "Renamed"]].concat())
        .await
        .unwrap();

    let row = stored(&ctx, model.id).await;
    assert_eq!(row.name, "Renamed");
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
    let id = model.id.to_string();
    let argv = ["gglib", "model", "update", &id, "--force", "--projector"];

    let refused = run(&ctx, &[&argv[..], &[weights.to_str().unwrap()]].concat())
        .await
        .unwrap_err()
        .to_string();

    assert!(refused.contains("mmproj-fake.gguf"), "{refused}");
    assert!(refused.contains("not a projector"), "{refused}");
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
