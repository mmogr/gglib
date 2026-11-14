//! Tests for the two drawing settings on `gglib config`: the
//! default image model and the `/mcp` drawing switch, set, shown and unset
//! over a database of the test's own.

use gglib_core::domain::{ImageFamily, NewModel};

use super::super::settings_display::settings_display_rows;
use super::super::unset::handle_unset;
use super::tests::key;
use super::*;
use crate::bootstrap::test_context;

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

/// The row `config show` prints for `key`.
pub(super) async fn shown(ctx: &CliContext, key: &str) -> String {
    let settings = ctx.app.settings().get().await.expect("settings");
    let (model, image) = resolve_model_display(ctx, &settings).await.expect("names");
    let rows = settings_display_rows(&settings, model.as_deref(), image.as_deref());
    let row = rows.into_iter().find(|(k, _)| k == key);
    row.unwrap_or_else(|| panic!("no {key} row")).1
}

/// The key takes a name or an id, stores the id, and `show` names the model.
#[tokio::test]
async fn the_default_image_model_is_set_by_name_or_id_and_shown_by_name() {
    let dir = tempfile::tempdir().expect("tempdir");
    let ctx = test_context(dir.path()).await;
    let flux = model(&ctx, "flux", true, dir.path()).await;
    let sdxl = model(&ctx, "sdxl", true, dir.path()).await;
    assert_eq!(shown(&ctx, "default-image-model-id").await, "None");

    handle_set(&ctx, key("default-image-model-id"), "flux")
        .await
        .expect("set by name");
    let stored = ctx.app.settings().get().await.expect("settings");
    assert_eq!(stored.default_image_model_id, Some(flux));
    assert_eq!(
        shown(&ctx, "default-image-model-id").await,
        format!("{flux} (flux)")
    );

    handle_set(&ctx, key("default-image-model-id"), &sdxl.to_string())
        .await
        .expect("set by id");
    let stored = ctx.app.settings().get().await.expect("settings");
    assert_eq!(stored.default_image_model_id, Some(sdxl));
}

/// A chat model is refused in the settings service's own sentence, the one
/// the HTTP API answers too, and nothing is stored.
#[tokio::test]
async fn a_model_that_does_not_draw_is_refused_in_one_sentence() {
    let dir = tempfile::tempdir().expect("tempdir");
    let ctx = test_context(dir.path()).await;
    let qwen = model(&ctx, "qwen", false, dir.path()).await;
    let before = ctx.app.settings().get().await.expect("settings");

    let refused = handle_set(&ctx, key("default-image-model-id"), "qwen")
        .await
        .expect_err("a chat model cannot draw");

    assert_eq!(
        format!("{refused:#}"),
        format!(
            "Model {qwen} (qwen) does not draw images, so it cannot be the default image model"
        )
    );
    assert_eq!(ctx.app.settings().get().await.expect("settings"), before);
}

/// A name the library does not hold is refused before anything is written.
#[tokio::test]
async fn a_name_the_library_does_not_hold_is_refused() {
    let dir = tempfile::tempdir().expect("tempdir");
    let ctx = test_context(dir.path()).await;
    let before = ctx.app.settings().get().await.expect("settings");

    let refused = handle_set(&ctx, key("default-image-model-id"), "nothing-here")
        .await
        .expect_err("no such model");

    assert!(
        format!("{refused:#}").contains("nothing-here"),
        "the refusal names what was asked for: {refused:#}"
    );
    assert_eq!(ctx.app.settings().get().await.expect("settings"), before);
}

/// The switch is off when absent and `show` says so; `true` turns it on,
/// only `true` and `false` parse, and `unset` turns it back off.
#[tokio::test]
async fn the_mcp_drawing_switch_is_off_when_absent_and_set_by_true_or_false() {
    let dir = tempfile::tempdir().expect("tempdir");
    let ctx = test_context(dir.path()).await;
    assert_eq!(shown(&ctx, "mcp-drawing").await, "None (off)");

    handle_set(&ctx, key("mcp-drawing"), "true")
        .await
        .expect("on");
    let stored = ctx.app.settings().get().await.expect("settings");
    assert_eq!(stored.mcp_drawing, Some(true));
    assert!(stored.effective_mcp_drawing());
    assert_eq!(shown(&ctx, "mcp-drawing").await, "true");

    handle_set(&ctx, key("mcp-drawing"), "false")
        .await
        .expect("off");
    let stored = ctx.app.settings().get().await.expect("settings");
    assert_eq!(stored.mcp_drawing, Some(false));

    handle_set(&ctx, key("mcp-drawing"), "maybe")
        .await
        .expect_err("only true or false is a switch position");

    handle_unset(&ctx, key("mcp-drawing")).await.expect("unset");
    let stored = ctx.app.settings().get().await.expect("settings");
    assert_eq!(stored.mcp_drawing, None);
    assert_eq!(shown(&ctx, "mcp-drawing").await, "None (off)");
}

/// `unset default-image-model-id` clears the default image model.
#[tokio::test]
async fn unset_clears_the_default_image_model() {
    let dir = tempfile::tempdir().expect("tempdir");
    let ctx = test_context(dir.path()).await;
    model(&ctx, "flux", true, dir.path()).await;
    handle_set(&ctx, key("default-image-model-id"), "flux")
        .await
        .expect("set");

    handle_unset(&ctx, key("default-image-model-id"))
        .await
        .expect("unset");

    let stored = ctx.app.settings().get().await.expect("settings");
    assert_eq!(stored.default_image_model_id, None);
}
