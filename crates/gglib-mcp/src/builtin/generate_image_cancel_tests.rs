//! A call of the drawing tool that is dropped drops its render.
//!
//! The tool awaits the driver's `generate` inside its own future, so a run
//! that is cancelled while a picture is drawn (the loop drops the call)
//! drops the render with it, and the driver stops a render nobody reads.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use gglib_core::ToolCall;
use gglib_core::domain::agent::{ToolProgressSink, ToolProgressUpdate};
use gglib_core::ports::{
    ImageBatch, ImageError, ImageGenerationPort, ImageProgress, ImageRequest, ImageStage,
    ToolExecutorPort,
};
use serde_json::json;
use tokio::sync::mpsc;

use super::{DrawArm, DrawingTool};
use crate::builtin::BuiltinToolExecutorAdapter;
use crate::tool_images::tool_images_tests::images;

/// Counts itself when dropped.
struct Left(Arc<AtomicUsize>);

impl Drop for Left {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

/// A driver whose render never ends: it counts each render it was asked
/// for and each one whose reader left.
#[derive(Debug, Default)]
struct NeverDone {
    asked: AtomicUsize,
    left: Arc<AtomicUsize>,
}

#[async_trait]
impl ImageGenerationPort for NeverDone {
    async fn generate(
        &self,
        _request: ImageRequest,
        progress: mpsc::Sender<ImageProgress>,
    ) -> Result<ImageBatch, ImageError> {
        let _held = Left(Arc::clone(&self.left));
        self.asked.fetch_add(1, Ordering::SeqCst);
        let _ = progress
            .send(ImageProgress::stage(ImageStage::Loading))
            .await;
        std::future::pending().await
    }
}

/// Counts the updates it is sent.
#[derive(Default)]
struct Counted(AtomicUsize);

impl ToolProgressSink for Counted {
    fn progress(&self, _update: ToolProgressUpdate) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

/// Wait, for at most five seconds, until `count` reads `want`.
async fn reaches(count: &AtomicUsize, want: usize) -> bool {
    for _ in 0..500 {
        if count.load(Ordering::SeqCst) == want {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    false
}

#[tokio::test]
async fn a_dropped_call_of_the_drawing_tool_drops_its_render() {
    let driver = Arc::new(NeverDone::default());
    let tool = DrawingTool::new(
        Arc::clone(&driver) as Arc<dyn ImageGenerationPort>,
        images().0,
    );
    let adapter =
        BuiltinToolExecutorAdapter::default().with_drawing(Some((tool, DrawArm::Fixed(true))));
    let heard = Arc::new(Counted::default());
    let call = ToolCall {
        id: "c1".to_owned(),
        name: "builtin:generate_image".to_owned(),
        arguments: json!({"prompt": "a fox"}),
    };
    let sink = Arc::clone(&heard);
    let drawing =
        tokio::spawn(async move { adapter.execute_with_progress(&call, sink.as_ref()).await });
    assert!(reaches(&heard.0, 1).await, "the render is under way");
    assert_eq!(driver.left.load(Ordering::SeqCst), 0, "and is being read");

    drawing.abort();
    assert!(drawing.await.unwrap_err().is_cancelled());

    assert!(
        reaches(&driver.left, 1).await,
        "the render's reader left with the call"
    );
    assert_eq!(
        driver.asked.load(Ordering::SeqCst),
        1,
        "one render was asked"
    );
}
