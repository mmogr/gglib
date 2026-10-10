//! Tests for the two drawing settings at the HTTP settings API's layer:
//! [`super::SettingsOps`] over a real database, driven with the JSON the page
//! sends.

use std::sync::Arc;

use super::*;
use crate::test_support::{MockDownloadManager, MockSystemProbePort, test_core};

fn make_ops(core: Arc<AppCore>) -> SettingsOps {
    SettingsOps::new(SettingsDeps {
        core,
        system_probe: Arc::new(MockSystemProbePort::default()),
        downloads: Arc::new(MockDownloadManager::new()),
    })
}

fn request(json: &str) -> UpdateSettingsRequest {
    serde_json::from_str(json).expect("a request")
}

/// Registers `name`, drawing when `draws`, and answers its id.
async fn model(core: &AppCore, name: &str, draws: bool) -> i64 {
    let mut new = gglib_core::domain::NewModel::new(
        name.to_owned(),
        std::path::PathBuf::from(format!("/models/{name}.gguf")),
        8.0,
        chrono::Utc::now(),
    );
    if draws {
        new.image_family = Some(gglib_core::domain::ImageFamily::Flux1);
    }
    core.models().add(new).await.expect("registered").id
}

/// The wire spells both in camelCase, distinguishes an omitted key from
/// `null`, and the settings it answers carry both, `null` when unset.
#[test]
fn both_ride_the_wire_in_camel_case_with_null_meaning_clear() {
    let set = request(r#"{"defaultImageModelId": 2, "mcpDrawing": true}"#);
    assert_eq!(set.default_image_model_id, Some(Some(2)));
    assert_eq!(set.mcp_drawing, Some(Some(true)));

    let clear = request(r#"{"defaultImageModelId": null, "mcpDrawing": null}"#);
    assert_eq!(clear.default_image_model_id, Some(None));
    assert_eq!(clear.mcp_drawing, Some(None));

    let left = request("{}");
    assert_eq!(left.default_image_model_id, None);
    assert_eq!(left.mcp_drawing, None);

    let wire = serde_json::to_value(AppSettings::default()).unwrap();
    assert_eq!(wire["defaultImageModelId"], serde_json::Value::Null);
    assert_eq!(wire["mcpDrawing"], serde_json::Value::Null);
}

/// An image model is stored through the API and read back by it.
#[tokio::test]
async fn the_api_sets_and_reads_the_default_image_model() {
    let core = test_core().await;
    let flux = model(&core, "flux", true).await;
    let ops = make_ops(core);

    let body = format!(r#"{{"defaultImageModelId": {flux}}}"#);
    let answered = ops.update(request(&body)).await.expect("stored");

    assert_eq!(answered.default_image_model_id, Some(flux));
    assert_eq!(ops.get().await.unwrap().default_image_model_id, Some(flux));

    let cleared = ops.update(request(r#"{"defaultImageModelId": null}"#));
    assert_eq!(cleared.await.unwrap().default_image_model_id, None);
}

/// A chat model is refused as a validation failure, in the sentence that
/// names it, and nothing is stored.
#[tokio::test]
async fn the_api_refuses_a_model_that_does_not_draw() {
    let core = test_core().await;
    let qwen = model(&core, "qwen", false).await;
    let ops = make_ops(core);

    let body = format!(r#"{{"defaultImageModelId": {qwen}, "mcpDrawing": true}}"#);
    let refused = ops.update(request(&body)).await.unwrap_err();

    let GuiError::ValidationFailed(sentence) = refused else {
        panic!("a validation failure: {refused:?}");
    };
    assert_eq!(
        sentence,
        format!(
            "Model {qwen} (qwen) does not draw images, so it cannot be the default image model"
        )
    );
    let after = ops.get().await.unwrap();
    assert_eq!(after.default_image_model_id, None);
    assert_eq!(after.mcp_drawing, None);
}

/// The switch reads as unset on a fresh install, which is off; the API turns
/// it on, and `null` turns it back off.
#[tokio::test]
async fn the_api_sets_and_reads_the_mcp_drawing_switch() {
    let ops = make_ops(test_core().await);
    assert_eq!(ops.get().await.unwrap().mcp_drawing, None);

    let on = ops
        .update(request(r#"{"mcpDrawing": true}"#))
        .await
        .unwrap();
    assert_eq!(on.mcp_drawing, Some(true));
    assert_eq!(ops.get().await.unwrap().mcp_drawing, Some(true));

    let off = ops
        .update(request(r#"{"mcpDrawing": null}"#))
        .await
        .unwrap();
    assert_eq!(off.mcp_drawing, None);
}
