//! `builtin:generate_image`: the chat model draws, through this machine's
//! image driver, only for a message the person sent with Draw pressed.
//!
//! The tool takes the prompt the model writes from the message, and a size,
//! a count and a seed when it wants them; never a model, which is the
//! driver's to choose (the default image model, or the only complete one).
//! While the render runs, each stage and step goes to the progress sink,
//! with its preview frame. Each image is stored as an attachment on the
//! tool's row, so the person sees it; the model reads one sentence saying
//! what was drawn, and never the image. A refusal or a failed render is a
//! result the model reads (`success: false`), not a fault of the loop.
//!
//! Whether the tool is offered at all is [`DrawArm`]'s: armed for a run sent
//! with `draw: true`, or for the next message after `/draw` in `gglib chat`.

use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use gglib_core::domain::agent::{ToolProgressSink, ToolProgressUpdate, ToolStage};
use gglib_core::ports::{
    ImageBatch, ImageError, ImageGenerationPort, ImageProgress, ImageRequest, ImageSize,
    ImageStage, MAX_IMAGES_PER_REQUEST,
};
use gglib_core::services::AttachmentService;
use gglib_core::{McpTool, ToolCall, ToolResult};
use serde_json::{Value, json};
use tokio::sync::mpsc;

/// The tool's name, unprefixed.
pub(crate) const NAME: &str = "generate_image";

/// Whether the drawing tool is offered right now.
#[derive(Debug, Clone)]
pub enum DrawArm {
    /// Fixed for the life of a run: `true` for one sent with `draw: true`.
    Fixed(bool),
    /// A session's switch, set by `/draw` and cleared after each send.
    Shared(Arc<AtomicBool>),
}

impl DrawArm {
    /// Whether the tool may be listed and called.
    #[must_use]
    pub fn is_armed(&self) -> bool {
        match self {
            Self::Fixed(armed) => *armed,
            Self::Shared(armed) => armed.load(Ordering::SeqCst),
        }
    }
}

impl Default for DrawArm {
    /// Not armed.
    fn default() -> Self {
        Self::Fixed(false)
    }
}

/// Draws through `images`, storing what it draws through `attachments`.
#[derive(Clone)]
pub struct DrawingTool {
    images: Arc<dyn ImageGenerationPort>,
    attachments: Arc<AttachmentService>,
}

impl fmt::Debug for DrawingTool {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DrawingTool")
            .field("images", &self.images)
            .finish_non_exhaustive()
    }
}

impl DrawingTool {
    /// A tool over this machine's image driver (or the daemon's, for the
    /// CLI) and the store the run's images go to.
    #[must_use]
    pub fn new(images: Arc<dyn ImageGenerationPort>, attachments: Arc<AttachmentService>) -> Self {
        Self {
            images,
            attachments,
        }
    }

    /// Draw what `call` asks for, telling `sink` how far it has got.
    pub(crate) async fn draw(&self, call: &ToolCall, sink: &dyn ToolProgressSink) -> ToolResult {
        let request = match request_of(&call.arguments) {
            Ok(request) => request,
            Err(why) => return ToolResult::text(call.id.clone(), why, false),
        };
        match self.render(request, sink).await {
            Ok(batch) => self.store(call, &batch).await,
            Err(e) => ToolResult::text(call.id.clone(), e.to_string(), false),
        }
    }

    /// Run the render, passing each report to `sink` as it comes.
    async fn render(
        &self,
        request: ImageRequest,
        sink: &dyn ToolProgressSink,
    ) -> Result<ImageBatch, ImageError> {
        let (reports, mut rx) = mpsc::channel(64);
        let work = self.images.generate(request, reports);
        tokio::pin!(work);
        let result = loop {
            tokio::select! {
                biased;
                Some(progress) = rx.recv() => sink.progress(update_of(progress)),
                result = &mut work => break result,
            }
        };
        while let Ok(progress) = rx.try_recv() {
            sink.progress(update_of(progress));
        }
        result
    }

    /// Store each image and say in one sentence what was drawn.
    async fn store(&self, call: &ToolCall, batch: &ImageBatch) -> ToolResult {
        let mut images = Vec::new();
        let mut refusals = Vec::new();
        for image in &batch.images {
            match self.attachments.ingest(&image.bytes).await {
                Ok(upload) => images.push(upload.info),
                Err(e) => refusals.push(e.to_string()),
            }
        }
        let content = drew_sentence(batch, images.len(), &refusals);
        ToolResult {
            images,
            ..ToolResult::text(
                call.id.clone(),
                content,
                refusals.len() < batch.images.len(),
            )
        }
    }
}

/// "Drew 1 image, 1024x1024 PNG, with flux in 76 s; the user can see it, you
/// cannot." With any image refused by the store, what became of it; with
/// none stored, that the user cannot see them either.
pub(crate) fn drew_sentence(batch: &ImageBatch, stored: usize, refusals: &[String]) -> String {
    let drawn = batch.images.len();
    let plural = if drawn == 1 { "image" } else { "images" };
    let size = batch.images.first().map_or_else(String::new, |image| {
        let format = image
            .mime
            .strip_prefix("image/")
            .unwrap_or(image.mime)
            .to_uppercase();
        format!(", {}x{} {format}", image.width, image.height)
    });
    let secs = batch.elapsed.as_secs_f64().round();
    let head = format!(
        "Drew {drawn} {plural}{size}, with {} in {secs:.0} s",
        batch.model
    );
    if refusals.is_empty() {
        let them = if drawn == 1 { "it" } else { "them" };
        return format!("{head}; the user can see {them}, you cannot.");
    }
    let why = refusals.join("; ");
    if stored == 0 {
        return format!("{head}, but none could be stored, so the user cannot see them: {why}");
    }
    format!(
        "{head}; the user can see {stored} of them, you cannot. {} could not be stored: {why}",
        refusals.len()
    )
}

/// The request `arguments` ask for, or what is wrong with them, in words the
/// model can act on.
pub(crate) fn request_of(arguments: &Value) -> Result<ImageRequest, String> {
    let prompt = arguments
        .get("prompt")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .ok_or_else(|| {
            "generate_image needs a prompt: describe the picture in detail".to_owned()
        })?;
    let size = match arguments.get("size").and_then(Value::as_str) {
        Some(text) => Some(text.parse::<ImageSize>().map_err(|e| e.to_string())?),
        None => None,
    };
    let n = match arguments.get("n").and_then(Value::as_u64) {
        Some(n) => u8::try_from(n)
            .ok()
            .filter(|n| (1..=MAX_IMAGES_PER_REQUEST).contains(n))
            .ok_or_else(|| format!("n is from 1 to {MAX_IMAGES_PER_REQUEST}, not {n}"))?,
        None => 1,
    };
    Ok(ImageRequest {
        model: None,
        prompt: prompt.to_owned(),
        size,
        n,
        seed: arguments.get("seed").and_then(Value::as_i64),
    })
}

/// A render's report, as the tool's progress.
pub(crate) fn update_of(progress: ImageProgress) -> ToolProgressUpdate {
    let mut update = match progress.stage {
        ImageStage::Queued { position, .. } => ToolProgressUpdate {
            position: Some(position),
            ..ToolProgressUpdate::stage(ToolStage::Queued)
        },
        ImageStage::Loading => ToolProgressUpdate::stage(ToolStage::Loading),
        ImageStage::Sampling { pass, step, total } => ToolProgressUpdate {
            pass: Some(pass),
            done: Some(step),
            total: Some(total),
            ..ToolProgressUpdate::stage(ToolStage::Sampling)
        },
        ImageStage::Decoding => ToolProgressUpdate::stage(ToolStage::Decoding),
        ImageStage::Finishing => ToolProgressUpdate::stage(ToolStage::Finishing),
    };
    update.preview = progress.preview;
    update
}

/// The tool as the model is offered it.
pub(crate) fn definition() -> McpTool {
    McpTool::new(NAME)
        .with_description(
            "The user pressed Draw for this message. Call this once with a detailed prompt you \
             write from their message; then say in one sentence what you drew.",
        )
        .with_input_schema(json!({
            "type": "object",
            "properties": {
                "prompt": {
                    "type": "string",
                    "description": "A detailed description of the picture: subject, setting, \
                                    style, lighting, composition."
                },
                "size": {
                    "type": "string",
                    "description": "WIDTHxHEIGHT, such as 1024x1024; leave it out for the \
                                    model's default square."
                },
                "n": {
                    "type": "integer",
                    "minimum": 1,
                    "maximum": MAX_IMAGES_PER_REQUEST,
                    "description": "How many images; leave it out for one."
                },
                "seed": {
                    "type": "integer",
                    "description": "A seed, to draw the same picture again; leave it out for \
                                    a random one."
                }
            },
            "required": ["prompt"]
        }))
}

#[cfg(test)]
#[path = "generate_image_tests.rs"]
mod tests;
