//! The drawing tool: offered only when armed, refused otherwise; a render's
//! reports become the tool's progress; what it drew is stored and named in
//! one sentence; a refusal is a result the model reads.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use gglib_core::ToolCall;
use gglib_core::domain::agent::{PreviewFrame, ToolProgressSink, ToolProgressUpdate, ToolStage};
use gglib_core::ports::{
    GeneratedImage, IMAGE_JOB_DEADLINE, ImageBatch, ImageError, ImageGenerationPort, ImageProgress,
    ImageRequest, ImageStage, ToolExecutorPort,
};
use serde_json::{Value, json};
use tokio::sync::mpsc;

use super::{DrawArm, DrawingTool, drew_sentence, request_of};
use crate::builtin::BuiltinToolExecutorAdapter;
use crate::tool_images::tool_images_tests::images;

const DRAW: &str = "builtin:generate_image";

/// A PNG's signature and `IHDR` at `width` by `height`.
fn png(width: u32, height: u32) -> Vec<u8> {
    let mut bytes = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 13];
    bytes.extend(b"IHDR");
    bytes.extend(width.to_be_bytes());
    bytes.extend(height.to_be_bytes());
    bytes.extend([8, 6, 0, 0, 0, 0, 0, 0, 0]);
    bytes
}

fn image(bytes: Vec<u8>) -> GeneratedImage {
    GeneratedImage {
        bytes,
        mime: "image/png",
        width: 1024,
        height: 1024,
    }
}

fn batch(images: Vec<GeneratedImage>) -> ImageBatch {
    ImageBatch {
        model: "flux1-schnell".to_owned(),
        images,
        elapsed: Duration::from_millis(75_600),
    }
}

/// A driver that sends `reports`, then answers `result`, keeping each
/// request it was asked.
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
            asked: Mutex::default(),
        })
    }

    fn drawing(self: &Arc<Self>) -> DrawingTool {
        DrawingTool::new(Arc::clone(self) as Arc<dyn ImageGenerationPort>, images().0)
    }

    fn asked(&self) -> usize {
        self.asked.lock().unwrap().len()
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
            progress.send(report.clone()).await.unwrap();
        }
        self.result.clone()
    }
}

/// Keeps every update it is sent.
#[derive(Default)]
struct Heard(Mutex<Vec<ToolProgressUpdate>>);

impl ToolProgressSink for Heard {
    fn progress(&self, update: ToolProgressUpdate) {
        self.0.lock().unwrap().push(update);
    }
}

fn call(arguments: Value) -> ToolCall {
    ToolCall {
        id: "c1".to_owned(),
        name: DRAW.to_owned(),
        arguments,
    }
}

fn adapter(driver: &Arc<Scripted>, armed: DrawArm) -> BuiltinToolExecutorAdapter {
    BuiltinToolExecutorAdapter::default().with_drawing(Some((driver.drawing(), armed)))
}

async fn names(adapter: &BuiltinToolExecutorAdapter) -> Vec<String> {
    adapter
        .list_tools()
        .await
        .into_iter()
        .map(|t| t.name)
        .collect()
}

fn one_image() -> Arc<Scripted> {
    Scripted::new(Vec::new(), Ok(batch(vec![image(png(1024, 1024))])))
}

// ── offered only when armed ──────────────────────────────────────────────

/// Listed only with a drawing tool that is armed; with its deadline, which
/// is the driver's; a session's switch is read at each listing.
#[tokio::test]
async fn generate_image_is_listed_only_when_armed() {
    let driver = one_image();
    assert!(
        !names(&BuiltinToolExecutorAdapter::default())
            .await
            .contains(&DRAW.to_owned())
    );
    let no_tool = BuiltinToolExecutorAdapter::default().with_drawing(None);
    assert!(!names(&no_tool).await.contains(&DRAW.to_owned()));
    assert!(
        !names(&adapter(&driver, DrawArm::Fixed(false)))
            .await
            .contains(&DRAW.to_owned())
    );

    let armed = adapter(&driver, DrawArm::Fixed(true)).list_tools().await;
    let draw = armed
        .iter()
        .find(|t| t.name == DRAW)
        .expect("listed when armed");
    assert_eq!(draw.deadline, Some(IMAGE_JOB_DEADLINE));
    let schema = draw.input_schema.as_ref().expect("a schema");
    assert_eq!(schema["required"], json!(["prompt"]));
    assert!(
        schema["properties"].get("model").is_none(),
        "no model argument"
    );

    let switch = Arc::new(AtomicBool::new(false));
    let session = adapter(&driver, DrawArm::Shared(Arc::clone(&switch)));
    assert!(!names(&session).await.contains(&DRAW.to_owned()));
    switch.store(true, Ordering::SeqCst);
    assert!(names(&session).await.contains(&DRAW.to_owned()));
    switch.store(false, Ordering::SeqCst);
    assert!(!names(&session).await.contains(&DRAW.to_owned()));
    assert_eq!(driver.asked(), 0);
}

/// Called when not armed, by `execute` or with progress, it is refused and
/// the driver is never asked.
#[tokio::test]
async fn generate_image_is_refused_when_not_armed() {
    let driver = one_image();
    for unarmed in [
        BuiltinToolExecutorAdapter::default(),
        adapter(&driver, DrawArm::Fixed(false)),
        adapter(&driver, DrawArm::Shared(Arc::new(AtomicBool::new(false)))),
    ] {
        let refused = unarmed.execute(&call(json!({"prompt": "a fox"}))).await;
        assert!(refused.unwrap_err().to_string().contains("Draw pressed"));
        let refused = unarmed
            .execute_with_progress(&call(json!({"prompt": "a fox"})), &Heard::default())
            .await;
        assert!(refused.is_err());
    }
    assert_eq!(driver.asked(), 0);
}

/// The page's tool list never carries it: a tool there can be switched on
/// and sent with any message.
#[test]
fn the_pages_builtin_list_never_carries_generate_image() {
    let names: Vec<String> = BuiltinToolExecutorAdapter::bare_definitions()
        .into_iter()
        .map(|t| t.name)
        .collect();
    assert!(
        !names.iter().any(|n| n.contains("generate_image")),
        "{names:?}"
    );
    assert!(names.contains(&"get_current_time".to_owned()));
}

// ── a render ─────────────────────────────────────────────────────────────

/// What it drew is stored, and the model reads one sentence: the count, the
/// size and format, the model and the seconds, and that it cannot see it.
#[tokio::test]
async fn a_render_is_stored_and_named_in_one_sentence() {
    let driver = one_image();
    let adapter = adapter(&driver, DrawArm::Fixed(true));

    let result = adapter
        .execute_with_progress(
            &call(json!({"prompt": "a fox", "model": "sdxl"})),
            &Heard::default(),
        )
        .await
        .unwrap();

    assert!(result.success);
    assert_eq!(
        result.content,
        "Drew 1 image, 1024x1024 PNG, with flux1-schnell in 76 s; the user can see it, you \
         cannot."
    );
    assert_eq!(result.images.len(), 1);
    assert_eq!(result.tool_call_id, "c1");
    let asked = driver.asked.lock().unwrap().clone();
    assert_eq!(asked[0].prompt, "a fox");
    assert_eq!(asked[0].model, None, "the model chooses no image model");
}

/// An image the store refuses is said to be unseen; with none stored, the
/// result is a failure the model can tell the user about.
#[tokio::test]
async fn an_image_the_store_refuses_is_said_to_be_unseen() {
    let driver = Scripted::new(Vec::new(), Ok(batch(vec![image(b"not an image".to_vec())])));
    let result = adapter(&driver, DrawArm::Fixed(true))
        .execute(&call(json!({"prompt": "a fox"})))
        .await
        .unwrap();
    assert!(!result.success);
    assert!(result.images.is_empty());
    assert!(
        result.content.starts_with(
            "Drew 1 image, 1024x1024 PNG, with flux1-schnell in 76 s, but none could be stored, \
             so the user cannot see them: "
        ),
        "{}",
        result.content
    );

    let some = drew_sentence(
        &batch(vec![image(Vec::new()), image(Vec::new())]),
        1,
        &["too large".to_owned()],
    );
    assert_eq!(
        some,
        "Drew 2 images, 1024x1024 PNG, with flux1-schnell in 76 s; the user can see 1 of them, \
         you cannot. 1 could not be stored: too large"
    );
}

/// A refusal or a failed render is a result in the driver's words, not a
/// fault of the loop.
#[tokio::test]
async fn a_refused_render_is_a_result_in_the_drivers_words() {
    let driver = Scripted::new(
        Vec::new(),
        Err(ImageError::Unavailable {
            reason: "there is no image model on this machine".to_owned(),
        }),
    );
    let result = adapter(&driver, DrawArm::Fixed(true))
        .execute(&call(json!({"prompt": "a fox"})))
        .await
        .unwrap();
    assert!(!result.success);
    assert_eq!(result.content, "there is no image model on this machine");
}

/// Arguments the driver could not be asked are a result saying what to fix.
#[test]
fn arguments_are_read_or_refused_in_words_the_model_can_act_on() {
    let read =
        request_of(&json!({"prompt": " a fox ", "size": "768x1024", "n": 2, "seed": 7})).unwrap();
    assert_eq!(
        (
            read.prompt.as_str(),
            read.size.map(|s| s.to_string()),
            read.n,
            read.seed
        ),
        ("a fox", Some("768x1024".to_owned()), 2, Some(7))
    );
    assert_eq!(request_of(&json!({"prompt": "a"})).unwrap().n, 1);
    assert!(
        request_of(&json!({}))
            .unwrap_err()
            .contains("needs a prompt")
    );
    assert!(
        request_of(&json!({"prompt": "a", "n": 5}))
            .unwrap_err()
            .contains("from 1 to 4")
    );
    assert!(
        request_of(&json!({"prompt": "a", "size": "big"}))
            .unwrap_err()
            .contains("WIDTHxHEIGHT")
    );
}

/// Each stage of the render reaches the sink as the tool's progress, in
/// order, a step with its pass and frame, a place in line with its position.
#[tokio::test]
async fn each_stage_of_the_render_is_the_tools_progress() {
    let frame = PreviewFrame::png(3, 20, "f3");
    let reports = vec![
        ImageProgress::stage(ImageStage::Queued {
            position: 2,
            behind: Some("an image render".to_owned()),
        }),
        ImageProgress::stage(ImageStage::Loading),
        ImageProgress {
            stage: ImageStage::Sampling {
                pass: 1,
                step: 3,
                total: 20,
            },
            preview: Some(frame.clone()),
        },
        ImageProgress::stage(ImageStage::Decoding),
        ImageProgress::stage(ImageStage::Finishing),
    ];
    let driver = Scripted::new(reports, Ok(batch(vec![image(png(1024, 1024))])));
    let heard = Heard::default();

    adapter(&driver, DrawArm::Fixed(true))
        .execute_with_progress(&call(json!({"prompt": "a fox"})), &heard)
        .await
        .unwrap();

    let heard = heard.0.into_inner().unwrap();
    assert_eq!(
        heard,
        [
            ToolProgressUpdate {
                position: Some(2),
                ..ToolProgressUpdate::stage(ToolStage::Queued)
            },
            ToolProgressUpdate::stage(ToolStage::Loading),
            ToolProgressUpdate {
                pass: Some(1),
                done: Some(3),
                total: Some(20),
                preview: Some(frame),
                ..ToolProgressUpdate::stage(ToolStage::Sampling)
            },
            ToolProgressUpdate::stage(ToolStage::Decoding),
            ToolProgressUpdate::stage(ToolStage::Finishing),
        ]
    );
}
