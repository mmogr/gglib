//! Which image model the tool draws with: the one its composer chose, by
//! id, on the request the driver reads; never one the calling model names,
//! and nothing of it in what the model is offered.

use serde_json::json;

use super::super::{DrawWith, definition};
use super::{DrawArm, Heard, adapter, call, one_image};
use crate::builtin::BuiltinToolExecutorAdapter;
use gglib_core::ports::ToolExecutorPort as _;

/// The chosen id is the request's model; with none chosen the request names
/// none, and the driver's default rule draws.
#[tokio::test]
async fn the_chosen_image_model_is_the_requests_model() {
    let driver = one_image();
    let chosen = DrawWith::model(Some(7));
    let tool = driver.drawing().drawing_with(chosen.clone());
    let adapter =
        BuiltinToolExecutorAdapter::default().with_drawing(Some((tool, DrawArm::Fixed(true))));
    let draw = call(json!({"prompt": "a fox"}));

    adapter
        .execute_with_progress(&draw, &Heard::default())
        .await
        .unwrap();
    // Changed between calls, as `/draw <model>` and `/draw default` do.
    chosen.set(Some(9));
    adapter
        .execute_with_progress(&draw, &Heard::default())
        .await
        .unwrap();
    chosen.set(None);
    adapter
        .execute_with_progress(&draw, &Heard::default())
        .await
        .unwrap();

    let asked: Vec<Option<String>> = driver
        .asked
        .lock()
        .unwrap()
        .iter()
        .map(|request| request.model.clone())
        .collect();
    assert_eq!(asked, [Some("7".to_owned()), Some("9".to_owned()), None]);
}

/// A `model` the calling model sends is not read: with a choice the choice
/// draws, and with none the request names none.
#[tokio::test]
async fn a_model_argument_never_replaces_the_choice() {
    let driver = one_image();
    let tool = driver.drawing().drawing_with(DrawWith::model(Some(7)));
    let chosen =
        BuiltinToolExecutorAdapter::default().with_drawing(Some((tool, DrawArm::Fixed(true))));

    chosen
        .execute_with_progress(
            &call(json!({"prompt": "a fox", "model": "9", "image_model": 9})),
            &Heard::default(),
        )
        .await
        .unwrap();
    adapter(&driver, DrawArm::Fixed(true))
        .execute_with_progress(
            &call(json!({"prompt": "a fox", "model": "9", "image_model": 9})),
            &Heard::default(),
        )
        .await
        .unwrap();

    let asked = driver.asked.lock().unwrap().clone();
    assert_eq!(asked[0].model.as_deref(), Some("7"));
    assert_eq!(asked[1].model, None);
}

/// The tool as the model is offered it has no argument for the image model,
/// with or without a choice: its properties are the four it always had.
#[tokio::test]
async fn the_offered_tool_has_no_model_argument() {
    let schema = definition().input_schema.expect("a schema");
    let mut keys: Vec<&str> = schema["properties"]
        .as_object()
        .expect("properties")
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    assert_eq!(keys, ["n", "prompt", "seed", "size"]);

    let driver = one_image();
    let tool = driver.drawing().drawing_with(DrawWith::model(Some(4242)));
    let adapter =
        BuiltinToolExecutorAdapter::default().with_drawing(Some((tool, DrawArm::Fixed(true))));
    let listed = serde_json::to_string(&adapter.list_tools().await).unwrap();
    assert!(!listed.contains("4242"), "the id is in no tool offered");
    assert!(
        !listed.contains("\"model\""),
        "no model property is offered"
    );
    let _ = adapter;
}
