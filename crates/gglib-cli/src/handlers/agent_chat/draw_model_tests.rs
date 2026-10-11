//! `/draw <model>`, `/draw default` and `/draw list`: the session's choice
//! of image model, made among the daemon's, held across messages, placed on
//! the request the image tool hands the daemon, and never swapped for the
//! default when it can no longer draw.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use gglib_core::ToolCall;
use gglib_core::contracts::http::images::{DrawingAvailability, ImageModelRow};
use gglib_core::domain::agent::NoProgress;
use gglib_core::domain::{ComponentRole, ImageFamily};
use gglib_core::ports::{
    ImageBatch, ImageError, ImageGenerationPort, ImageProgress, ImageRequest, ToolExecutorPort,
};
use gglib_mcp::BuiltinToolExecutorAdapter;
use serde_json::json;
use tokio::sync::mpsc;

use super::DrawSwitch;
use crate::bootstrap::test_context;

const QWEN: u32 = 7;
const SCHNELL: u32 = 3;
const DEV: u32 = 4;
const SDXL: u32 = 9;

fn row(id: u32, name: &str, family: ImageFamily) -> ImageModelRow {
    ImageModelRow {
        id,
        name: name.to_owned(),
        family,
        complete: true,
        missing: Vec::new(),
        default: false,
        resident: false,
    }
}

/// A daemon with four image models, in the order it lists them: one
/// Qwen-Image; two Flux.1, the first the default and loaded; one SDXL with
/// no VAE. It judges a chosen id as a daemon does, keeps each request to
/// draw, and draws nothing. `gone` takes a model away after the fact.
#[derive(Debug, Default)]
struct Library {
    gone: Mutex<Option<u32>>,
    asked: Mutex<Vec<ImageRequest>>,
}

impl Library {
    fn rows(&self) -> Vec<ImageModelRow> {
        let gone = *self.gone.lock().unwrap();
        let rows = vec![
            row(QWEN, "qwen-image-q8", ImageFamily::QwenImage21),
            ImageModelRow {
                default: true,
                resident: true,
                ..row(SCHNELL, "flux1-schnell", ImageFamily::Flux1)
            },
            row(DEV, "flux1-dev", ImageFamily::Flux1),
            ImageModelRow {
                complete: false,
                missing: vec![ComponentRole::Vae],
                ..row(SDXL, "sdxl-base", ImageFamily::Sdxl)
            },
        ];
        rows.into_iter()
            .filter(|row| Some(row.id) != gone)
            .collect()
    }
}

#[async_trait]
impl ImageGenerationPort for Library {
    async fn generate(
        &self,
        request: ImageRequest,
        _progress: mpsc::Sender<ImageProgress>,
    ) -> Result<ImageBatch, ImageError> {
        self.asked.lock().unwrap().push(request);
        Err(ImageError::Failed {
            message: "a test's driver draws nothing".to_owned(),
        })
    }

    async fn drawing(&self, chosen: Option<u32>) -> DrawingAvailability {
        let rows = self.rows();
        let id = chosen.unwrap_or(SCHNELL);
        let answer = match rows.iter().find(|row| row.id == id) {
            None => DrawingAvailability::refused(
                "model_not_found",
                format!("the image model chosen (id {id}) is no longer on this machine"),
            ),
            Some(row) if !row.complete => DrawingAvailability::refused(
                "image_model_incomplete",
                format!("{} has no vae linked", row.name),
            ),
            Some(row) => DrawingAvailability {
                model_id: Some(row.id),
                ..DrawingAvailability::with(row.name.clone())
            },
        };
        answer.listing(rows)
    }
}

fn session() -> (Arc<Library>, DrawSwitch) {
    let library = Arc::new(Library::default());
    let switch = DrawSwitch::through(Arc::clone(&library) as Arc<dyn ImageGenerationPort>);
    (library, switch)
}

const HINT: &str = "  /draw <name or id> chooses one for this session; /draw default forgets it";

/// `/draw list` prints the daemon's rows in its order and arms nothing: the
/// star is on the one the next `/draw` message would draw with, and a
/// session's choice is marked on its row.
#[tokio::test]
async fn draw_list_prints_the_daemons_image_models_and_arms_nothing() {
    let (_, switch) = session();

    let listed = switch.say(Some("list")).await;

    let rows = [
        "     7  qwen-image-q8  (Qwen-Image 2.1)",
        "*    3  flux1-schnell  (Flux.1, default, loaded)",
        "     4  flux1-dev  (Flux.1)",
        "     9  sdxl-base  (SDXL, incomplete, missing VAE)",
        HINT,
    ];
    assert_eq!(listed, rows.join("\n"));
    assert!(!switch.is_armed());
    assert_eq!(switch.chosen(), None);

    switch.say(Some("qwen")).await;
    switch.sent();
    let listed = switch.say(Some("list")).await;
    let rows = [
        "*    7  qwen-image-q8  (Qwen-Image 2.1, this session's)",
        "     3  flux1-schnell  (Flux.1, default, loaded)",
        "     4  flux1-dev  (Flux.1)",
        "     9  sdxl-base  (SDXL, incomplete, missing VAE)",
        HINT,
    ];
    assert_eq!(listed, rows.join("\n"));
    assert!(!switch.is_armed(), "listing arms nothing");
}

/// `/draw <model>` chooses by id, by whole name, or by a part only one
/// model has, whatever the case, arms the next message and says so.
#[tokio::test]
async fn draw_with_a_model_chooses_it_by_name_or_id_and_arms() {
    for (named, id, name) in [
        ("7", QWEN, "qwen-image-q8"),
        ("QWEN", QWEN, "qwen-image-q8"),
        ("flux1-dev", DEV, "flux1-dev"),
        ("schnell", SCHNELL, "flux1-schnell"),
    ] {
        let (_, switch) = session();
        let said = switch.say(Some(named)).await;
        assert_eq!(
            said,
            format!("the next message draws, with {name}, this session's image model"),
            "{named}"
        );
        assert_eq!(switch.chosen(), Some(id), "{named}");
        assert!(switch.is_armed(), "{named}");
    }
}

/// A name that fits several, or none, and a model that lacks a file, change
/// nothing and arm nothing: the first two are answered with the list, the
/// last with the daemon's reason.
#[tokio::test]
async fn a_name_that_does_not_choose_one_drawable_model_changes_nothing() {
    let (_, switch) = session();
    switch.say(Some("qwen")).await;
    switch.sent();

    let several = switch.say(Some("flux1")).await;
    assert!(
        several.starts_with("`flux1` fits 2 image models; say more of the name, or its id\n"),
        "{several}"
    );
    assert!(
        several.contains("flux1-dev") && several.ends_with(HINT),
        "{several}"
    );

    let none = switch.say(Some("dall-e")).await;
    assert!(
        none.starts_with("no image model here is named or numbered `dall-e`\n"),
        "{none}"
    );
    assert!(
        none.contains("qwen-image-q8  (Qwen-Image 2.1, this session's)"),
        "{none}"
    );

    let lacking = switch.say(Some("sdxl")).await;
    assert_eq!(
        lacking,
        "cannot draw with sdxl-base: sdxl-base has no vae linked"
    );

    assert_eq!(switch.chosen(), Some(QWEN), "the choice stands");
    assert!(!switch.is_armed());
}

/// The choice is the session's: it survives the message it was made for,
/// a bare `/draw` says it, and each picture is asked of the daemon with its
/// id as the request's model, which the chat model is never shown.
/// `/draw default` forgets it, and the next picture names no model.
#[tokio::test]
async fn the_choice_is_held_for_the_session_and_goes_on_each_request_to_draw() {
    let dir = tempfile::tempdir().expect("tempdir");
    let ctx = test_context(dir.path()).await;
    let (library, switch) = session();
    let tools = BuiltinToolExecutorAdapter::default().with_drawing(switch.tool(&ctx));
    let call = ToolCall {
        id: "c1".to_owned(),
        name: "builtin:generate_image".to_owned(),
        arguments: json!({ "prompt": "a fox", "model": "flux1-dev" }),
    };
    let draw = || async {
        tools
            .execute_with_progress(&call, &NoProgress)
            .await
            .unwrap();
        switch.sent();
    };

    switch.say(Some("qwen")).await;
    let offered = serde_json::to_string(&tools.list_tools().await).unwrap();
    assert!(offered.contains("generate_image"), "{offered}");
    assert!(
        !offered.contains("qwen") && !offered.contains("\"model\""),
        "{offered}"
    );
    draw().await;
    assert!(!switch.is_armed());
    assert_eq!(switch.chosen(), Some(QWEN), "kept after its message");

    let said = switch.say(None).await;
    assert_eq!(
        said,
        "the next message draws, with qwen-image-q8, this session's image model"
    );
    draw().await;

    let said = switch.say(Some("default")).await;
    assert_eq!(said, "the next message draws, with flux1-schnell");
    assert_eq!(switch.chosen(), None);
    assert!(switch.is_armed());
    draw().await;

    let models: Vec<Option<String>> = library
        .asked
        .lock()
        .unwrap()
        .iter()
        .map(|request| request.model.clone())
        .collect();
    assert_eq!(models, [Some("7".to_owned()), Some("7".to_owned()), None]);
}

/// A chosen model that has since gone is not swapped for the default: a
/// bare `/draw` says why it cannot draw and how to choose again, arms
/// nothing and keeps the choice.
#[tokio::test]
async fn a_choice_that_can_no_longer_draw_is_refused_not_replaced() {
    let (library, switch) = session();
    switch.say(Some("qwen")).await;
    switch.sent();
    *library.gone.lock().unwrap() = Some(QWEN);

    let said = switch.say(None).await;

    assert_eq!(
        said,
        "cannot draw with this session's image model: the image model chosen (id 7) is no \
         longer on this machine\n  /draw <model> chooses another, /draw default the daemon's \
         default, /draw list shows them"
    );
    assert!(!switch.is_armed());
    assert_eq!(switch.chosen(), Some(QWEN));

    let listed = switch.say(Some("list")).await;
    assert!(
        listed.contains("  nothing would draw: the image model chosen (id 7)"),
        "{listed}"
    );
    assert!(!listed.contains('*'), "no row would draw: {listed}");
}
