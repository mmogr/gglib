//! The keys `config set` and `unset` take, held to what `config show`
//! prints and to the wire type they write through.

use std::collections::BTreeSet;

use clap::Parser as _;
use gglib_core::Settings;
use gglib_core::domain::{ImageFamily, NewModel};

use super::set::tests::key;
use super::settings_display::{camel_to_kebab, settings_display_rows};
use super::wire::known_keys;
use super::{handle_set, handle_unset, shown_rows};
use crate::bootstrap::{CliContext, test_context};
use crate::config_commands::{Bound, KEYS, Takes};

/// The setting a printed row belongs to: `remote-pairing.ticket` is
/// `remote-pairing`'s.
fn top_level(row: &str) -> String {
    row.split('.').next().unwrap_or(row).to_owned()
}

/// Registers `name`, drawing when `draws`, and answers its id.
async fn model(ctx: &CliContext, name: &str, draws: bool, dir: &std::path::Path) -> i64 {
    let mut new = NewModel::new(
        name.to_owned(),
        dir.join(format!("{name}.gguf")),
        Some(8.0),
        chrono::Utc::now(),
    );
    if draws {
        new.image_family = Some(ImageFamily::Flux1);
    }
    ctx.app.models().add(new).await.expect("registered").id
}

/// The value `config show` prints for `name`.
async fn row(ctx: &CliContext, name: &str) -> String {
    let rows = shown_rows(ctx).await.expect("rows");
    let found = rows.into_iter().find(|(key, _)| *key == name);
    found.unwrap_or_else(|| panic!("no {name} row")).1
}

/// The table holds every key `config show` prints, every key the wire type
/// carries and `models-dir`, and nothing else. A key another command writes
/// is exactly one `show` prints that the wire type cannot write, so a field
/// added to either surface and left out of the table fails here.
#[test]
fn the_table_is_what_show_prints_and_the_wire_type_carries() {
    let wire: BTreeSet<String> = known_keys().keys().map(|k| camel_to_kebab(k)).collect();
    let printed: BTreeSet<String> = settings_display_rows(&Settings::default(), None, None)
        .into_iter()
        .map(|(key, _)| top_level(&key))
        .collect();
    let table: BTreeSet<String> = KEYS.iter().map(|key| key.name.to_owned()).collect();

    let mut expected = &wire | &printed;
    expected.insert("models-dir".to_owned());
    assert_eq!(table, expected);
    for key in KEYS {
        let elsewhere = matches!(key.takes, Takes::Elsewhere(_));
        let written = wire.contains(key.name) || key.name == "models-dir";
        assert_eq!(elsewhere, !written, "{}", key.name);
    }
}

/// Whether `gglib config <command> <name>` gets past clap.
fn parses(command: &str, name: &str) -> bool {
    crate::Cli::try_parse_from(["gglib", "config", command, name, "x"])
        .or_else(|_| crate::Cli::try_parse_from(["gglib", "config", command, name]))
        .is_ok()
}

/// Every key `config show` prints is one `config set` and `config unset`
/// take, set to a value of its kind, shown changed, and cleared back to how
/// it was shown; or one they do not take, which clap refuses and their
/// `--help` names with where it is changed.
#[tokio::test]
async fn every_key_show_prints_is_taken_by_set_and_unset() {
    gglib_core::paths::isolate_data_root();
    let dir = tempfile::tempdir().expect("tempdir");
    let ctx = test_context(dir.path()).await;
    model(&ctx, "flux", true, dir.path()).await;
    let folder = dir.path().join("models here");

    let rows = shown_rows(&ctx).await.expect("rows");
    let printed: BTreeSet<String> = rows.iter().map(|(key, _)| top_level(key)).collect();
    assert!(printed.contains("models-dir"), "{printed:?}");

    for (at, name) in (0u64..).zip(&printed) {
        let key = super::set::tests::key(name);
        assert_eq!(parses("set", name), key.settable(), "set {name}");
        assert_eq!(parses("unset", name), key.clearable(), "unset {name}");
        let typed = match key.takes {
            Takes::Number(Bound::Closed(lo, _) | Bound::Clamped(lo, _)) => lo.to_string(),
            // Apart from each other, so two ports never collide.
            Takes::Number(Bound::AtLeast(lo)) => (lo + 1000 + at).to_string(),
            Takes::Number(Bound::Unbounded) => "3".to_owned(),
            Takes::Switch => "true".to_owned(),
            Takes::Text => "127.0.0.1".to_owned(),
            Takes::Hosts => "gglib.lan".to_owned(),
            Takes::Model => "flux".to_owned(),
            Takes::Folder => folder.display().to_string(),
            Takes::Whole(_) | Takes::Elsewhere(_) => "anything".to_owned(),
        };
        let before = row(&ctx, name).await;
        let set = handle_set(&ctx, key, &typed).await;

        match key.takes {
            Takes::Elsewhere(_) => {
                let said = format!("{:#}", set.expect_err(name));
                assert!(said.contains("is not set by hand: `gglib remote"), "{said}");
                let said = format!("{:#}", handle_unset(&ctx, key).await.expect_err(name));
                assert!(said.contains("is not set by hand: `gglib remote"), "{said}");
            }
            Takes::Whole(_) => {
                let said = format!("{:#}", set.expect_err(name));
                assert!(said.contains("is not set from one value"), "{said}");
                handle_unset(&ctx, key).await.expect(name);
            }
            _ => {
                // `GGLIB_MODELS_DIR` in this test's environment outranks the
                // stored folder, so the row cannot change under it.
                let shown =
                    key.takes != Takes::Folder || std::env::var_os("GGLIB_MODELS_DIR").is_none();
                set.unwrap_or_else(|e| panic!("set {name} {typed}: {e:#}"));
                if shown {
                    assert_ne!(row(&ctx, name).await, before, "{name} {typed}");
                }
                handle_unset(&ctx, key)
                    .await
                    .unwrap_or_else(|e| panic!("unset {name}: {e:#}"));
                assert_eq!(row(&ctx, name).await, before, "{name}");
            }
        }
    }
}

/// `default-model-id` takes a model by name or id, as every command naming
/// a model does, stores the id and shows the name; one the library does not
/// hold is refused in the resolver's words, and nothing is stored.
#[tokio::test]
async fn the_default_model_is_set_by_name_or_id_and_an_unknown_one_refused() {
    let dir = tempfile::tempdir().expect("tempdir");
    let ctx = test_context(dir.path()).await;
    let qwen = model(&ctx, "qwen", false, dir.path()).await;
    let llama = model(&ctx, "llama", false, dir.path()).await;

    handle_set(&ctx, key("default-model-id"), "qwen")
        .await
        .expect("by name");
    let stored = ctx.app.settings().get().await.expect("settings");
    assert_eq!(stored.default_model_id, Some(qwen));
    let rows = settings_display_rows(&stored, Some("shown"), None);
    assert!(rows.contains(&("default-model-id".to_owned(), "shown".to_owned())));

    handle_set(&ctx, key("default-model-id"), &llama.to_string())
        .await
        .expect("by id");
    let refused = handle_set(&ctx, key("default-model-id"), "__no_such_model__")
        .await
        .expect_err("no such model");
    assert!(
        format!("{refused:#}").contains("No model found matching: '__no_such_model__'"),
        "{refused:#}"
    );
    let stored = ctx.app.settings().get().await.expect("settings");
    assert_eq!(stored.default_model_id, Some(llama));
}
