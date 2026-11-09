//! The Draw switch: armed by `/draw` only when the daemon can draw, for one
//! message, and never for a session that has nothing to draw with.

use std::sync::Arc;

use async_trait::async_trait;
use gglib_core::ports::{ImageBatch, ImageError, ImageGenerationPort, ImageProgress, ImageRequest};
use tokio::sync::mpsc;

use super::DrawSwitch;
use crate::daemon_client::DaemonHandle;
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

    async fn drawing(
        &self,
        _chosen: Option<u32>,
    ) -> gglib_core::contracts::http::images::DrawingAvailability {
        match self.0 {
            Ok(model) => gglib_core::contracts::http::images::DrawingAvailability::with(model),
            Err(reason) => gglib_core::contracts::http::images::DrawingAvailability::refused(
                "drawing_unavailable",
                reason,
            ),
        }
    }
}

/// `/draw` arms the next message and says which model draws: that
/// message's turn says `draw`. Once it is sent the switch is off again, and
/// the turn after it does not.
#[tokio::test]
async fn draw_arms_one_message_and_is_off_after_it_is_sent() {
    let switch = DrawSwitch::through(Arc::new(Says(Ok("flux1-schnell"))));
    assert!(!switch.is_armed());
    assert_eq!(switch.turn(), (false, None), "no draw before /draw");

    let said = switch.arm().await;

    assert_eq!(said, "the next message draws, with flux1-schnell");
    assert_eq!(switch.turn(), (true, None), "the next message draws");

    switch.sent();

    assert!(!switch.is_armed());
    assert_eq!(switch.turn(), (false, None), "the message after does not");
}

/// A daemon that cannot draw arms nothing, and `/draw` says why.
#[tokio::test]
async fn draw_says_why_when_the_daemon_cannot_draw() {
    let switch = DrawSwitch::through(Arc::new(Says(Err("there is no image model"))));
    assert_eq!(switch.arm().await, "cannot draw: there is no image model");
    assert!(!switch.is_armed());
}

/// A `--remote` session cannot draw and says its model is elsewhere,
/// asking the daemon nothing.
#[tokio::test]
async fn a_remote_session_cannot_draw_and_says_why() {
    let daemon = DaemonHandle {
        client: gglib_proxy::loopback::client(),
        api_key: None,
    };

    let far = DrawSwitch::for_session(&daemon, Target::Remote);

    let said = far.arm().await;
    assert!(said.contains("another machine"), "{said}");
    assert!(!far.is_armed());
    assert_eq!(far.turn(), (false, None));
}

/// A local session with a daemon draws through it: `/draw` asks that
/// daemon's `/api/images/drawing`.
#[tokio::test]
async fn a_local_session_asks_its_daemon_whether_it_can_draw() {
    use crate::daemon_client::STAND_IN_PORT;
    use crate::daemon_client::handle_tests::{answering, daemon_health};

    let answer = r#"{"available":true,"model":"sdxl"}"#.to_owned();
    let (daemon, asked) = answering(daemon_health(), answer);
    let handle = DaemonHandle {
        client: gglib_proxy::loopback::client(),
        api_key: None,
    };

    let said = STAND_IN_PORT
        .scope(daemon, async {
            let switch = DrawSwitch::for_session(&handle, Target::Local);
            let said = switch.arm().await;
            assert!(switch.is_armed());
            said
        })
        .await;

    assert_eq!(said, "the next message draws, with sdxl");
    let asked = asked.lock().unwrap().clone();
    assert!(
        asked
            .iter()
            .any(|(line, _)| line.contains("GET /api/images/drawing")),
        "{asked:?}"
    );
}
