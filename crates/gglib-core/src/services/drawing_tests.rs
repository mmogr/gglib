//! The availability table: each reason, and the order they are asked in.

use std::sync::atomic::{AtomicUsize, Ordering};

use async_trait::async_trait;
use tokio::sync::mpsc;

use super::drawing_availability;
use crate::contracts::http::images::DrawingAvailability;
use crate::ports::{ImageBatch, ImageError, ImageGenerationPort, ImageProgress, ImageRequest};

/// A driver that answers `drawing_model` with what it was given, counting
/// how often it was asked.
#[derive(Debug)]
struct Answers {
    model: Result<String, String>,
    asked: AtomicUsize,
}

impl Answers {
    fn can(model: &str) -> Self {
        Self {
            model: Ok(model.to_owned()),
            asked: AtomicUsize::new(0),
        }
    }

    fn cannot(reason: &str) -> Self {
        Self {
            model: Err(reason.to_owned()),
            asked: AtomicUsize::new(0),
        }
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

    async fn drawing_model(&self) -> Result<String, ImageError> {
        self.asked.fetch_add(1, Ordering::SeqCst);
        self.model
            .clone()
            .map_err(|reason| ImageError::Unavailable { reason })
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
    answer.reason.as_deref().unwrap()
}

#[tokio::test]
async fn a_complete_image_model_makes_drawing_available_and_names_it() {
    let driver = Answers::can("flux1-schnell");
    let answer = drawing_availability(Some(&driver), false, Some(true)).await;
    assert_eq!(answer, DrawingAvailability::with("flux1-schnell"));
    let unknown = drawing_availability(Some(&driver), false, None).await;
    assert!(unknown.available, "a model not known to refuse tools draws");
}

#[tokio::test]
async fn a_far_model_is_refused_first_without_asking_the_driver() {
    let driver = Answers::cannot("no image model");
    let answer = drawing_availability(Some(&driver), true, Some(false)).await;
    assert!(reason(&answer).contains("another machine"), "{answer:?}");
    assert_eq!(driver.asked.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn a_model_that_calls_no_tools_is_refused_before_the_driver_is_asked() {
    let driver = Answers::cannot("no image model");
    let answer = drawing_availability(Some(&driver), false, Some(false)).await;
    assert!(reason(&answer).contains("calls no tools"), "{answer:?}");
    assert_eq!(driver.asked.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn no_driver_is_refused() {
    let answer = drawing_availability(None, false, Some(true)).await;
    assert!(
        reason(&answer).contains("nothing to draw with"),
        "{answer:?}"
    );
}

#[tokio::test]
async fn the_drivers_reason_is_the_answer() {
    let driver = Answers::cannot("the image runtime is not installed");
    let answer = drawing_availability(Some(&driver), false, Some(true)).await;
    assert_eq!(reason(&answer), "the image runtime is not installed");
    assert_eq!(driver.asked.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn a_driver_that_cannot_say_offers_no_drawing() {
    let answer = drawing_availability(Some(&Silent), false, Some(true)).await;
    assert!(reason(&answer).contains("cannot say"), "{answer:?}");
}

#[test]
fn the_wire_leaves_out_what_an_answer_does_not_have() {
    let can = serde_json::to_value(DrawingAvailability::with("sdxl")).unwrap();
    assert_eq!(can, serde_json::json!({"available": true, "model": "sdxl"}));
    let cannot =
        serde_json::to_value(DrawingAvailability::refused("drawing_unavailable", "why")).unwrap();
    assert_eq!(
        cannot,
        serde_json::json!({"available": false, "code": "drawing_unavailable", "reason": "why"})
    );
}
