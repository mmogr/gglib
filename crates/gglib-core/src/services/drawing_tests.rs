//! The availability table: each reason, and the order they are asked in.

use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use async_trait::async_trait;
use tokio::sync::mpsc;

use super::model_tests::{CHAT, Catalog, FLUX, SDXL, TURBO, library, one_complete, spec};
use super::{drawing_availability, drawing_refusal, image_model_answer};
use crate::contracts::http::images::{DrawingAvailability, ImageModelRow};
use crate::domain::{ComponentRole, ImageFamily};
use crate::ports::{
    ImageBatch, ImageError, ImageGenerationPort, ImageProgress, ImageRequest, ModelRuntimeError,
};

/// A driver that answers `drawing` with what it was given, counting how
/// often it was asked and keeping what was chosen.
#[derive(Debug)]
struct Answers {
    answer: DrawingAvailability,
    asked: AtomicUsize,
    chosen: Mutex<Vec<Option<u32>>>,
}

impl Answers {
    fn new(answer: DrawingAvailability) -> Self {
        Self {
            answer,
            asked: AtomicUsize::new(0),
            chosen: Mutex::default(),
        }
    }

    fn can(model: &str) -> Self {
        Self::new(DrawingAvailability::with(model))
    }

    fn cannot(reason: &str) -> Self {
        Self::new(DrawingAvailability::refused("drawing_unavailable", reason))
    }
}

#[async_trait]
impl ImageGenerationPort for Answers {
    async fn generate(
        &self,
        _request: ImageRequest,
        _progress: mpsc::Sender<ImageProgress>,
    ) -> Result<ImageBatch, ImageError> {
        unreachable!("availability never draws")
    }

    async fn drawing(&self, chosen: Option<u32>) -> DrawingAvailability {
        self.asked.fetch_add(1, Ordering::SeqCst);
        self.chosen.lock().unwrap().push(chosen);
        self.answer.clone()
    }
}

/// A driver that keeps the default answer.
#[derive(Debug)]
struct Silent;

#[async_trait]
impl ImageGenerationPort for Silent {
    async fn generate(
        &self,
        _request: ImageRequest,
        _progress: mpsc::Sender<ImageProgress>,
    ) -> Result<ImageBatch, ImageError> {
        unreachable!("availability never draws")
    }
}

fn reason(answer: &DrawingAvailability) -> &str {
    assert!(!answer.available, "{answer:?}");
    assert_eq!(answer.code.as_deref(), Some("drawing_unavailable"));
    assert_eq!(answer.model, None);
    assert!(answer.models.is_empty(), "{answer:?}");
    answer.reason.as_deref().unwrap()
}

#[tokio::test]
async fn a_complete_image_model_makes_drawing_available_and_names_it() {
    let driver = Answers::can("flux1-schnell");
    let answer = drawing_availability(Some(&driver), false, Some(true), None).await;
    assert_eq!(answer, DrawingAvailability::with("flux1-schnell"));
    let unknown = drawing_availability(Some(&driver), false, None, None).await;
    assert!(unknown.available, "a model not known to refuse tools draws");
}

#[tokio::test]
async fn a_far_model_is_refused_first_without_asking_the_driver() {
    let driver = Answers::cannot("no image model");
    let answer = drawing_availability(Some(&driver), true, Some(false), Some(7)).await;
    assert!(reason(&answer).contains("another machine"), "{answer:?}");
    assert_eq!(driver.asked.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn a_model_that_calls_no_tools_is_refused_before_the_driver_is_asked() {
    let driver = Answers::cannot("no image model");
    let answer = drawing_availability(Some(&driver), false, Some(false), Some(7)).await;
    assert!(reason(&answer).contains("calls no tools"), "{answer:?}");
    assert_eq!(driver.asked.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn no_driver_is_refused() {
    let answer = drawing_availability(None, false, Some(true), Some(7)).await;
    assert!(
        reason(&answer).contains("nothing to draw with"),
        "{answer:?}"
    );
}

#[tokio::test]
async fn the_drivers_reason_is_the_answer() {
    let driver = Answers::cannot("the image runtime is not installed");
    let answer = drawing_availability(Some(&driver), false, Some(true), None).await;
    assert_eq!(reason(&answer), "the image runtime is not installed");
    assert_eq!(driver.asked.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn a_driver_that_cannot_say_offers_no_drawing() {
    let answer = drawing_availability(Some(&Silent), false, Some(true), None).await;
    assert!(reason(&answer).contains("cannot say"), "{answer:?}");
}

/// The chosen image model reaches the driver as it was asked, and its
/// answer comes back whole: code, reason and rows.
#[tokio::test]
async fn the_chosen_image_model_is_the_drivers_to_judge() {
    let refused = DrawingAvailability::refused("image_model_incomplete", "flux lacks vae")
        .listing(vec![row(FLUX, "flux", ImageFamily::Flux1)]);
    let driver = Answers::new(refused.clone());
    let answer = drawing_availability(Some(&driver), false, Some(true), Some(FLUX)).await;
    assert_eq!(answer, refused);
    drawing_availability(Some(&driver), false, None, None).await;
    assert_eq!(*driver.chosen.lock().unwrap(), [Some(FLUX), None]);
}

#[test]
fn the_wire_leaves_out_what_an_answer_does_not_have() {
    let can = serde_json::to_value(DrawingAvailability::with("sdxl")).unwrap();
    assert_eq!(
        can,
        serde_json::json!({"available": true, "model": "sdxl", "models": []})
    );
    let cannot =
        serde_json::to_value(DrawingAvailability::refused("drawing_unavailable", "why")).unwrap();
    assert_eq!(
        cannot,
        serde_json::json!({
            "available": false, "code": "drawing_unavailable", "reason": "why", "models": []
        })
    );
    let earlier: DrawingAvailability =
        serde_json::from_value(serde_json::json!({"available": true, "model": "sdxl"})).unwrap();
    assert_eq!(
        earlier,
        DrawingAvailability::with("sdxl"),
        "no list reads as none"
    );
}

// ── the driver's part: the model that draws, and the list ────────────────

/// A complete row that is neither the default nor loaded.
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

fn flux_row() -> ImageModelRow {
    ImageModelRow {
        complete: false,
        missing: vec![
            ComponentRole::Vae,
            ComponentRole::ClipL,
            ComponentRole::T5xxl,
        ],
        ..row(FLUX, "flux", ImageFamily::Flux1)
    }
}

/// Every image model is a row, the chat model is not; by family in the
/// recipes' order (Flux.1 before SDXL) and then by name; an incomplete one
/// says what it lacks; the default and the loaded one are marked, each on
/// its own row.
#[tokio::test]
async fn the_answer_lists_every_image_model_with_its_default_and_its_resident() {
    let answer = image_model_answer(&library(), Some(i64::from(TURBO)), None, &[SDXL, CHAT]).await;
    assert_eq!(
        answer,
        DrawingAvailability {
            model_id: Some(TURBO),
            ..DrawingAvailability::with("sdxl-turbo")
        }
        .listing(vec![
            flux_row(),
            ImageModelRow {
                resident: true,
                ..row(SDXL, "sdxl", ImageFamily::Sdxl)
            },
            ImageModelRow {
                default: true,
                ..row(TURBO, "sdxl-turbo", ImageFamily::Sdxl)
            },
        ])
    );
}

/// `default` marks what a request that names none draws with: the only
/// complete one when the settings name none, and no row when such a request
/// is refused (several complete, or a default that lacks a file). The rows
/// are listed all the same.
#[tokio::test]
async fn default_marks_what_an_unnamed_request_draws_with_or_nothing() {
    let defaults = |answer: &DrawingAvailability| {
        answer
            .models
            .iter()
            .filter(|row| row.default)
            .map(|row| row.id)
            .collect::<Vec<_>>()
    };
    let only = image_model_answer(&one_complete(), None, None, &[]).await;
    assert_eq!((only.model_id, defaults(&only)), (Some(SDXL), vec![SDXL]));

    let several = image_model_answer(&library(), None, None, &[]).await;
    assert_eq!(several.code.as_deref(), Some("drawing_unavailable"));
    assert_eq!((several.models.len(), defaults(&several)), (3, vec![]));

    let lacking = image_model_answer(&one_complete(), Some(i64::from(FLUX)), None, &[]).await;
    assert_eq!(lacking.code.as_deref(), Some("drawing_unavailable"));
    assert!(lacking.reason.as_deref().unwrap().contains("vae"));
    assert_eq!((lacking.models.len(), defaults(&lacking)), (2, vec![]));
}

/// A chosen model is judged by row 1 and never replaced: it draws although
/// the default is another and an unnamed request would be refused; gone,
/// chatting or lacking a file it is refused with that row's code, although
/// the default could draw. The rows, and the default's mark, stay as they
/// are whatever was chosen.
#[tokio::test]
async fn a_chosen_model_is_judged_as_a_request_naming_it_would_be() {
    let unnamed = image_model_answer(&library(), Some(i64::from(TURBO)), None, &[]).await;
    let ask = |chosen: u32| async move {
        image_model_answer(&library(), Some(i64::from(TURBO)), Some(chosen), &[]).await
    };

    let chosen = ask(SDXL).await;
    assert!(chosen.available);
    assert_eq!(
        (chosen.model.as_deref(), chosen.model_id),
        (Some("sdxl"), Some(SDXL))
    );
    assert_eq!(chosen.models, unnamed.models, "the default's mark stays");

    let no_default = image_model_answer(&library(), None, Some(SDXL), &[]).await;
    assert_eq!(no_default.model_id, Some(SDXL), "several, and one chosen");

    for (id, code, word) in [
        (
            99,
            "model_not_found",
            "(id 99) is no longer on this machine",
        ),
        (CHAT, "not_an_image_model", "qwen"),
        (FLUX, "image_model_incomplete", "--component vae=<path>"),
    ] {
        let refused = ask(id).await;
        assert!(!refused.available, "{refused:?}");
        assert_eq!(refused.code.as_deref(), Some(code));
        assert!(
            refused.reason.as_deref().unwrap().contains(word),
            "{refused:?}"
        );
        assert_eq!((refused.model.as_deref(), refused.model_id), (None, None));
        assert_eq!(refused.models, unnamed.models);
    }
}

/// A catalogue that cannot be read refuses with no rows.
#[tokio::test]
async fn a_catalogue_that_cannot_be_read_lists_nothing() {
    let broken = Catalog {
        broken: true,
        ..library()
    };
    let answer = image_model_answer(&broken, None, None, &[]).await;
    assert_eq!(answer.code.as_deref(), Some("drawing_unavailable"));
    assert!(answer.models.is_empty());
}

/// A model the catalogue lists as drawing but resolves without a family is
/// left out of the rows rather than shown with a made-up one.
#[tokio::test]
async fn a_row_needs_a_family() {
    let mut sdxl = spec(SDXL, "sdxl", Some(ImageFamily::Sdxl));
    let answer = image_model_answer(&Catalog::of(vec![sdxl.clone()]), None, None, &[]).await;
    assert_eq!(answer.models.len(), 1);
    sdxl.image_family = None;
    let answer = image_model_answer(&Catalog::of(vec![sdxl]), None, None, &[]).await;
    assert!(answer.models.is_empty());
}

/// The codes an answer refuses with are the ones a request to draw is
/// refused with.
#[test]
fn a_refusal_carries_the_code_a_request_would_be_refused_with() {
    let code = |e: ImageError| drawing_refusal(&e).code.unwrap();
    let runtime = ImageError::Runtime;
    assert_eq!(
        code(runtime(ModelRuntimeError::ModelNotFound("x".into()))),
        "model_not_found"
    );
    assert_eq!(
        code(runtime(ModelRuntimeError::ImageModelIncomplete {
            model: "flux".into(),
            missing: vec![ComponentRole::Vae],
        })),
        "image_model_incomplete"
    );
    assert_eq!(
        code(ImageError::NotAnImageModel { model: "q".into() }),
        "not_an_image_model"
    );
    assert_eq!(
        code(ImageError::Unavailable { reason: "r".into() }),
        "drawing_unavailable"
    );
    assert_eq!(
        code(runtime(ModelRuntimeError::Internal("db".into()))),
        "drawing_unavailable"
    );
}
