//! The Draw switch: armed by `/draw` only when the daemon can draw, for one
//! message, and never for a session that has nothing to draw with.

use std::sync::Arc;

use async_trait::async_trait;
use gglib_core::ports::{
    ImageBatch, ImageError, ImageGenerationPort, ImageProgress, ImageRequest, ToolExecutorPort,
};
use gglib_mcp::BuiltinToolExecutorAdapter;
use tokio::sync::mpsc;

use super::DrawSwitch;
use crate::bootstrap::test_context;
use crate::target::Target;

/// A driver that says it can draw, or why not.
#[derive(Debug)]
struct Says(Result<&'static str, &'static str>);

#[async_trait]
impl ImageGenerationPort for Says {
    async fn generate(
        &self,
        _request: ImageRequest,
        _progress: mpsc::Sender<ImageProgress>,
    ) -> Result<ImageBatch, ImageError> {
        unreachable!("the switch draws nothing")
    }

    async fn drawing_model(&self) -> Result<String, ImageError> {
        self.0
            .map(str::to_owned)
            .map_err(|reason| ImageError::Unavailable {
                reason: reason.to_owned(),
            })
    }
}

/// Whether the session's builtins list the image tool right now.
async fn listed(tools: &BuiltinToolExecutorAdapter) -> bool {
    tools
        .list_tools()
        .await
        .iter()
        .any(|tool| tool.name == "builtin:generate_image")
}

/// `/draw` arms the next message and says which model draws; once that
/// message is sent the switch is off again, and the tool with it.
#[tokio::test]
async fn draw_arms_one_message_and_is_off_after_it_is_sent() {
    let dir = tempfile::tempdir().expect("tempdir");
    let ctx = test_context(dir.path()).await;
    let switch = DrawSwitch::through(Arc::new(Says(Ok("flux1-schnell"))));
    let tools = BuiltinToolExecutorAdapter::default().with_drawing(switch.tool(&ctx));
    assert!(!switch.is_armed());
    assert!(!listed(&tools).await, "not offered before /draw");

    let said = switch.arm().await;

    assert_eq!(said, "the next message may draw, with flux1-schnell");
    assert!(switch.is_armed());
    assert!(listed(&tools).await, "offered for the next message");

    switch.sent();

    assert!(!switch.is_armed());
    assert!(!listed(&tools).await, "not offered to the message after");
}

/// A daemon that cannot draw arms nothing, and `/draw` says why.
#[tokio::test]
async fn draw_says_why_when_the_daemon_cannot_draw() {
    let switch = DrawSwitch::through(Arc::new(Says(Err("there is no image model"))));
    assert_eq!(switch.arm().await, "cannot draw: there is no image model");
    assert!(!switch.is_armed());
}

/// A `--remote` session has no image tool and says its model is elsewhere;
/// so does a session with no daemon, which is never started for this.
#[tokio::test]
async fn a_session_with_nothing_to_draw_with_has_no_tool_and_says_why() {
    let dir = tempfile::tempdir().expect("tempdir");
    let ctx = test_context(dir.path()).await;

    let far = DrawSwitch::for_session(&ctx, Target::Remote, None).await;
    assert!(far.tool(&ctx).is_none());
    let said = far.arm().await;
    assert!(said.contains("another machine"), "{said}");
    assert!(!far.is_armed());

    let alone = DrawSwitch::for_session(&ctx, Target::Local, Some(9)).await;
    assert!(alone.tool(&ctx).is_none());
    assert!(alone.arm().await.contains("no gglib daemon"));
}

/// A local session with a daemon draws through it: `/draw` asks that
/// daemon's `/api/images/drawing`.
#[tokio::test]
async fn a_local_session_asks_its_daemon_whether_it_can_draw() {
    use crate::daemon_client::STAND_IN_PORT;
    use crate::daemon_client::handle_tests::{answering, daemon_health};

    let dir = tempfile::tempdir().expect("tempdir");
    let ctx = test_context(dir.path()).await;
    let answer = r#"{"available":true,"model":"sdxl"}"#.to_owned();
    let (daemon, asked) = answering(daemon_health(), answer);

    let said = STAND_IN_PORT
        .scope(daemon, async {
            let switch = DrawSwitch::for_session(&ctx, Target::Local, Some(9)).await;
            assert!(switch.tool(&ctx).is_some());
            let said = switch.arm().await;
            assert!(switch.is_armed());
            said
        })
        .await;

    assert_eq!(said, "the next message may draw, with sdxl");
    let asked = asked.lock().unwrap().clone();
    assert!(
        asked
            .iter()
            .any(|(line, _)| line.contains("GET /api/images/drawing")),
        "{asked:?}"
    );
}
