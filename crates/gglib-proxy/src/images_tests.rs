//! The drawing route's handler against a scripted image port: its JSON
//! answer, every refusal's code and status, and the streamed form's events.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use axum::body::Bytes;
use axum::http::StatusCode;
use gglib_core::domain::ImageFamily;
use gglib_core::domain::agent::PreviewFrame;
use gglib_core::ports::{
    GeneratedImage, ImageBatch, ImageError, ImageGenerationPort, ImageProgress, ImageRequest,
    ImageSize, ImageStage, ModelRuntimeError,
};
use serde_json::{Value, json};
use tokio::sync::mpsc;

use super::generations;

/// Sends its reports, then answers its result; keeps every request.
#[derive(Debug)]
struct Scripted {
    reports: Vec<ImageProgress>,
    result: Result<ImageBatch, ImageError>,
    asked: Mutex<Vec<ImageRequest>>,
}

impl Scripted {
    fn new(reports: Vec<ImageProgress>, result: Result<ImageBatch, ImageError>) -> Arc<Self> {
        Arc::new(Self {
            reports,
            result,
            asked: Mutex::new(Vec::new()),
        })
    }
}

#[async_trait]
impl ImageGenerationPort for Scripted {
    async fn generate(
        &self,
        request: ImageRequest,
        progress: mpsc::Sender<ImageProgress>,
    ) -> Result<ImageBatch, ImageError> {
        self.asked.lock().unwrap().push(request);
        for report in &self.reports {
            let _ = progress.send(report.clone()).await;
        }
        self.result.clone()
    }
}

fn image(width: u32, height: u32) -> GeneratedImage {
    GeneratedImage {
        bytes: vec![1, 2, 3],
        mime: "image/png",
        width,
        height,
    }
}

fn batch(n: usize) -> ImageBatch {
    ImageBatch {
        model: "sdxl".to_owned(),
        images: (0..n).map(|_| image(1024, 1024)).collect(),
        elapsed: Duration::from_secs(76),
    }
}

fn sampling(pass: u32, step: u32, total: u32) -> ImageProgress {
    ImageProgress {
        stage: ImageStage::Sampling { pass, step, total },
        preview: Some(PreviewFrame::png(step, total, format!("f{pass}{step}"))),
    }
}

/// A render of `passes` images of four steps each, with every stage.
fn render(passes: u32) -> Vec<ImageProgress> {
    let mut reports = vec![
        ImageProgress::stage(ImageStage::Queued {
            position: 1,
            behind: Some("an image render".to_owned()),
        }),
        ImageProgress::stage(ImageStage::Loading),
    ];
    for pass in 1..=passes {
        reports.extend((1..=4).map(|step| sampling(pass, step, 4)));
    }
    reports.push(ImageProgress::stage(ImageStage::Decoding));
    reports.push(ImageProgress::stage(ImageStage::Finishing));
    reports
}

async fn post(port: Option<Arc<Scripted>>, body: &Value) -> (StatusCode, String) {
    let port = port.map(|p| p as Arc<dyn ImageGenerationPort>);
    let response = generations(port, Bytes::from(body.to_string())).await;
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    (status, String::from_utf8(bytes.to_vec()).unwrap())
}

/// Each server-sent event's `event:` name and JSON data.
fn events(text: &str) -> Vec<(String, Value)> {
    text.split("\n\n")
        .filter_map(|frame| {
            let name = frame.lines().find_map(|l| l.strip_prefix("event: "))?;
            let data = frame.lines().find_map(|l| l.strip_prefix("data: "))?;
            Some((name.to_owned(), serde_json::from_str(data).unwrap()))
        })
        .collect()
}

/// Without `stream`, the answer is `OpenAI`'s: `created`, each image as
/// base64, `output_format`; the request reaches the driver as asked.
#[tokio::test]
async fn a_request_is_answered_with_openais_shape() {
    let port = Scripted::new(render(1), Ok(batch(2)));
    let (status, body) = post(
        Some(Arc::clone(&port)),
        &json!({"model": "sdxl", "prompt": "a cat", "n": 2, "size": "1024x768",
                "seed": 9, "response_format": "b64_json", "quality": "high"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let body: Value = serde_json::from_str(&body).unwrap();
    assert!(body["created"].as_i64().unwrap() > 0);
    assert_eq!(body["output_format"], "png");
    assert_eq!(
        body["data"],
        json!([{"b64_json": "AQID"}, {"b64_json": "AQID"}])
    );
    assert_eq!(
        port.asked.lock().unwrap()[0],
        ImageRequest {
            model: Some("sdxl".to_owned()),
            prompt: "a cat".to_owned(),
            size: Some(ImageSize {
                width: 1024,
                height: 768
            }),
            n: 2,
            seed: Some(9),
        }
    );
}

/// `auto` and an absent size leave the size to the family.
#[tokio::test]
async fn an_automatic_size_is_the_familys() {
    let port = Scripted::new(Vec::new(), Ok(batch(1)));
    post(
        Some(Arc::clone(&port)),
        &json!({"prompt": "a", "size": "auto"}),
    )
    .await;
    assert_eq!(port.asked.lock().unwrap()[0].size, None);
}

/// Every refusal of the driver's answers its status and its code.
#[tokio::test]
async fn each_refusal_answers_its_status_and_code() {
    let rule = ImageFamily::Flux1.recipe().size;
    let driver = |e: ImageError| Some(Scripted::new(Vec::new(), Err(e)));
    let cases: Vec<(Option<Arc<Scripted>>, Value, StatusCode, &str)> = vec![
        (
            driver(ImageError::InvalidSize {
                width: 7,
                height: 7,
                rule,
            }),
            json!({"prompt": "a", "size": "7x7"}),
            StatusCode::BAD_REQUEST,
            "invalid_image_size",
        ),
        (
            driver(ImageError::NotAnImageModel {
                model: "qwen".into(),
            }),
            json!({"prompt": "a", "model": "qwen"}),
            StatusCode::BAD_REQUEST,
            "not_an_image_model",
        ),
        (
            driver(ImageError::Unavailable {
                reason: "no image model".into(),
            }),
            json!({"prompt": "a"}),
            StatusCode::BAD_REQUEST,
            "drawing_unavailable",
        ),
        (
            driver(ImageError::Failed {
                message: "boom".into(),
            }),
            json!({"prompt": "a"}),
            StatusCode::BAD_GATEWAY,
            "image_generation_failed",
        ),
        (
            driver(ImageError::Stalled {
                after: Duration::from_mins(3),
            }),
            json!({"prompt": "a"}),
            StatusCode::GATEWAY_TIMEOUT,
            "image_render_stalled",
        ),
        (
            driver(ImageError::Runtime(
                ModelRuntimeError::ImageRuntimeNotInstalled,
            )),
            json!({"prompt": "a"}),
            StatusCode::SERVICE_UNAVAILABLE,
            "image_runtime_not_installed",
        ),
        (
            None,
            json!({"prompt": "a"}),
            StatusCode::BAD_REQUEST,
            "drawing_unavailable",
        ),
    ];
    answers(cases).await;
}

/// What the route refuses itself never reaches the driver (whose answer
/// here would be a 504): an unreadable size, a format other than base64
/// PNG, too many images or partial images, no prompt.
#[tokio::test]
async fn the_route_refuses_what_no_driver_is_asked() {
    let driver = || Some(Scripted::new(Vec::new(), Err(ImageError::DeadlineExceeded)));
    let cases: Vec<(Option<Arc<Scripted>>, Value, StatusCode, &str)> = vec![
        (
            driver(),
            json!({"prompt": "a", "size": "big"}),
            StatusCode::BAD_REQUEST,
            "invalid_image_size",
        ),
        (
            driver(),
            json!({"prompt": "a", "response_format": "url"}),
            StatusCode::BAD_REQUEST,
            "invalid_request",
        ),
        (
            driver(),
            json!({"prompt": "a", "output_format": "webp"}),
            StatusCode::BAD_REQUEST,
            "invalid_request",
        ),
        (
            driver(),
            json!({"prompt": "a", "n": 9}),
            StatusCode::BAD_REQUEST,
            "invalid_request",
        ),
        (
            driver(),
            json!({"prompt": "a", "stream": true, "partial_images": 4}),
            StatusCode::BAD_REQUEST,
            "invalid_request",
        ),
        (
            driver(),
            json!({"n": 1}),
            StatusCode::BAD_REQUEST,
            "invalid_request",
        ),
    ];
    answers(cases).await;

    // Three partial images, the most there are, is asked of the driver.
    let asked = driver().unwrap();
    let most = json!({"prompt": "a", "partial_images": 3});
    let (status, text) = post(Some(Arc::clone(&asked)), &most).await;
    assert_eq!(status, StatusCode::GATEWAY_TIMEOUT, "{text}");
    assert_eq!(asked.asked.lock().unwrap().len(), 1);
}

async fn answers(cases: Vec<(Option<Arc<Scripted>>, Value, StatusCode, &str)>) {
    for (port, body, status, code) in cases {
        let (got, text) = post(port, &body).await;
        assert_eq!(got, status, "{body}: {text}");
        let text: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(text["error"]["code"], code, "{body}");
        assert!(
            !text["error"]["message"].as_str().unwrap().is_empty(),
            "{body}"
        );
    }
}

/// A stream sends progress for every stage, partial images only as many as
/// asked, and one completed event per image, last.
#[tokio::test]
async fn a_stream_reports_every_stage_caps_partials_and_completes_last() {
    let port = Scripted::new(render(2), Ok(batch(2)));
    let (status, text) = post(
        Some(port),
        &json!({"prompt": "a", "n": 2, "size": "1024x1024", "stream": true,
                "partial_images": 2}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let all = events(&text);
    let names: Vec<&str> = all.iter().map(|(n, _)| n.as_str()).collect();
    for (name, data) in &all {
        assert_eq!(data["type"], name.as_str(), "the name is the type");
    }
    let stages: Vec<&str> = all
        .iter()
        .filter(|(n, _)| n == "image_generation.progress")
        .map(|(_, d)| d["stage"].as_str().unwrap())
        .collect();
    assert_eq!(stages.first(), Some(&"queued"));
    for stage in ["queued", "loading", "sampling", "decoding", "finishing"] {
        assert!(stages.contains(&stage), "{stage} in {stages:?}");
    }
    assert_eq!(all[0].1["behind"], "an image render");
    let sampled = all.iter().find(|(_, d)| d["stage"] == "sampling").unwrap();
    assert_eq!(
        (&sampled.1["pass"], &sampled.1["step"], &sampled.1["total"]),
        (&json!(1), &json!(1), &json!(4))
    );
    assert_eq!(sampled.1["frame_b64"], "f11");

    let partials: Vec<&Value> = all
        .iter()
        .filter(|(n, _)| n == "image_generation.partial_image")
        .map(|(_, d)| d)
        .collect();
    assert_eq!(partials.len(), 2, "capped at partial_images");
    // Eight steps across two passes, two partials: steps 4 and 8.
    assert_eq!(partials[0]["b64_json"], "f14");
    assert_eq!(partials[1]["b64_json"], "f24");
    assert_eq!(
        (
            &partials[0]["partial_image_index"],
            &partials[1]["partial_image_index"]
        ),
        (&json!(0), &json!(1))
    );
    assert_eq!(partials[0]["size"], "1024x1024");

    assert_eq!(
        names[names.len() - 2..],
        ["image_generation.completed", "image_generation.completed"]
    );
    let done = &all.last().unwrap().1;
    assert_eq!(
        (&done["b64_json"], &done["size"], &done["output_format"]),
        (&json!("AQID"), &json!("1024x1024"), &json!("png"))
    );
    assert_eq!(done["model"], "sdxl", "the image model that drew it");
}

/// A stream that asks for no partial images gets none.
#[tokio::test]
async fn a_stream_without_partial_images_sends_none() {
    let port = Scripted::new(render(1), Ok(batch(1)));
    let (_, text) = post(Some(port), &json!({"prompt": "a", "stream": true})).await;
    assert!(
        !events(&text)
            .iter()
            .any(|(n, _)| n == "image_generation.partial_image")
    );
}

/// A render that fails partway ends its stream with one `error` event
/// carrying the body a response would.
#[tokio::test]
async fn a_failure_partway_ends_the_stream_with_its_error() {
    let port = Scripted::new(
        render(1),
        Err(ImageError::Stalled {
            after: Duration::from_mins(3),
        }),
    );
    let (status, text) = post(Some(port), &json!({"prompt": "a", "stream": true})).await;
    assert_eq!(status, StatusCode::OK, "the stream had started");
    let all = events(&text);
    let (name, data) = all.last().unwrap();
    assert_eq!(name, "error");
    assert_eq!(data["error"]["code"], "image_render_stalled");
    assert!(!all.iter().any(|(n, _)| n == "image_generation.completed"));
}

/// Partial images are spread across every pass, not each pass: one partial
/// of a two-image render falls on the last step of the second pass.
#[tokio::test]
async fn partial_images_are_spread_across_every_pass() {
    let port = Scripted::new(render(2), Ok(batch(2)));
    let (_, text) = post(
        Some(port),
        &json!({"prompt": "a", "n": 2, "stream": true, "partial_images": 1}),
    )
    .await;
    let partials: Vec<Value> = events(&text)
        .into_iter()
        .filter(|(n, _)| n == "image_generation.partial_image")
        .map(|(_, d)| d)
        .collect();
    assert_eq!(partials.len(), 1);
    assert_eq!(partials[0]["b64_json"], "f24");
}

/// A runtime refusal is the proxy's own mapping of it, `Retry-After`
/// included where retrying can help.
#[tokio::test]
async fn a_runtime_refusal_is_answered_as_the_proxy_answers_one() {
    let port = Scripted::new(
        Vec::new(),
        Err(ImageError::Runtime(
            ModelRuntimeError::ImageModelDoesNotFit {
                model: "flux".into(),
                held_model: "qwen".into(),
                needed_bytes: None,
                free_bytes: None,
            },
        )),
    );
    let response = generations(
        Some(port as Arc<dyn ImageGenerationPort>),
        Bytes::from(r#"{"prompt":"a"}"#),
    )
    .await;
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert!(response.headers().contains_key("retry-after"));
}
