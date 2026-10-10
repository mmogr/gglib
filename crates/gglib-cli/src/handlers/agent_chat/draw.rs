//! `/draw`: the session's Draw button.
//!
//! The image tool is offered to the model only for the message typed after
//! `/draw`, as the page offers it only for a message sent with its Draw
//! button pressed. [`DrawSwitch`] is that switch: `/draw` asks the daemon
//! whether it can draw and arms it, and it is cleared after each send, so a
//! model never starts a render on its own.
//!
//! The CLI never drives `sd-server` itself. The tool draws through the
//! daemon ([`DaemonImageGenerator`]), so a render queues with the daemon's
//! other work; a session with no daemon, or one whose model is on another
//! machine (`--remote`), cannot draw, and `/draw` says why.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use gglib_core::ports::ImageGenerationPort;
use gglib_core::services::{AttachmentService, drawing_availability};
use gglib_mcp::{DrawArm, DrawingTool};

use crate::bootstrap::CliContext;
use crate::daemon_client::{self, DaemonHandle, images::DaemonImageGenerator};
use crate::target::Target;

/// Why a `--remote` session cannot draw: its loop would run the tool here,
/// while the chat's model is there.
const FAR: &str = "this chat's model is on another machine, so it cannot draw here; a chat kept \
                   on that machine draws with its image model";

/// Why a `--port` session with no daemon cannot draw.
const NO_DAEMON: &str = "no gglib daemon is running to draw with; start one with `gglib daemon \
                         run`, then type /draw again";

/// A session's Draw switch.
#[derive(Debug)]
pub(crate) struct DrawSwitch {
    armed: Arc<AtomicBool>,
    /// What draws, or why nothing does.
    with: Result<Arc<dyn ImageGenerationPort>, String>,
}

impl DrawSwitch {
    /// The switch of a session on `target`, reusing a llama-server on `port`
    /// when one is named. A session the daemon starts a model for draws
    /// through that daemon; one on `--port` only through a daemon already
    /// running, and none is started for it.
    pub(crate) async fn for_session(ctx: &CliContext, target: Target, port: Option<u16>) -> Self {
        if target == Target::Remote {
            return Self::off(FAR);
        }
        // A test that names no stand-in daemon never reaches this machine's.
        #[cfg(test)]
        if daemon_client::STAND_IN_PORT.try_with(|_| ()).is_err() {
            return Self::off(NO_DAEMON);
        }
        let daemon = if port.is_some() {
            match daemon_client::running(ctx).await {
                Ok(daemon) => daemon,
                Err(_) => return Self::off(NO_DAEMON),
            }
        } else {
            DaemonHandle::new(ctx, gglib_proxy::loopback::client()).await
        };
        Self::through(Arc::new(DaemonImageGenerator::new(daemon)))
    }

    /// A switch that draws through `images`.
    pub(crate) fn through(images: Arc<dyn ImageGenerationPort>) -> Self {
        Self {
            armed: Arc::default(),
            with: Ok(images),
        }
    }

    /// A switch that never arms, for `reason`.
    pub(crate) fn off(reason: &str) -> Self {
        Self {
            armed: Arc::default(),
            with: Err(reason.to_owned()),
        }
    }

    /// The drawing tool this session's loop is composed with, armed only
    /// while this switch is; none for a session that cannot draw.
    pub(crate) fn tool(&self, ctx: &CliContext) -> Option<(DrawingTool, DrawArm)> {
        let images = self.with.as_ref().ok()?;
        let store = AttachmentService::new(ctx.app.attachments().store());
        Some((
            DrawingTool::new(Arc::clone(images), Arc::new(store)),
            DrawArm::Shared(Arc::clone(&self.armed)),
        ))
    }

    /// `/draw`: arm the next message when the daemon can draw, and say so;
    /// otherwise say why not, and arm nothing.
    pub(crate) async fn arm(&self) -> String {
        let images = match &self.with {
            Ok(images) => images,
            Err(reason) => return format!("cannot draw: {reason}"),
        };
        let answer = drawing_availability(Some(images.as_ref()), false, None).await;
        if !answer.available {
            let reason = answer.reason.unwrap_or_default();
            return format!("cannot draw: {reason}");
        }
        self.armed.store(true, Ordering::SeqCst);
        let model = answer.model.unwrap_or_default();
        format!("the next message may draw, with {model}")
    }

    /// A message was sent and its turn is over: the switch is off again.
    pub(crate) fn sent(&self) {
        self.armed.store(false, Ordering::SeqCst);
    }

    /// Whether the next message may draw.
    #[cfg(test)]
    pub(crate) fn is_armed(&self) -> bool {
        self.armed.load(Ordering::SeqCst)
    }
}

#[cfg(test)]
#[path = "draw_tests.rs"]
mod tests;
