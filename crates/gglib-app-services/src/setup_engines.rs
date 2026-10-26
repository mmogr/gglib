//! What an install or a removal of either engine must pass on the daemon:
//! no model running on that engine, and an install that is a download, not
//! the source build the command line would make. A `#[path]` child of
//! `setup.rs`, used by llama.cpp's operations there and the image runtime's
//! in `setup_image_runtime.rs` alike.

use gglib_core::domain::RuntimeKind;
use gglib_core::ports::ModelRuntimePort;
use gglib_runtime::engine::{InstallPlan, PinStanding, pin_state, plan_install};

use crate::error::GuiError;

/// The name of the model running on `kind`'s server, if one is.
pub(crate) async fn running_model(
    runtime: &dyn ModelRuntimePort,
    kind: RuntimeKind,
) -> Option<String> {
    runtime
        .list_running()
        .await
        .into_iter()
        .find(|handle| handle.runtime == kind)
        .map(|handle| handle.model_name)
}

/// A conflict when an install of `kind` would be a source build, which is
/// the command line's to run: a source build of a release is there, a GPU
/// build would otherwise become a CPU one, or this machine has no download.
/// The daemon installs downloads only.
pub(crate) fn refuse_a_source_build(kind: RuntimeKind) -> Result<(), GuiError> {
    let state = pin_state(kind).map_err(|e| GuiError::Internal(e.to_string()))?;
    let InstallPlan::Build { no_download } = plan_install(kind, &state, false, false) else {
        return Ok(());
    };
    let command = match kind {
        RuntimeKind::Llama => "gglib config llama update",
        RuntimeKind::StableDiffusion => "gglib config sd update",
    };
    let product = kind.label();
    Err(GuiError::Conflict(match no_download {
        Some(reason) => format!(
            "{}. Build {product} from a terminal: run '{command}'.",
            reason.trim_end_matches('.')
        ),
        // A record of a source build with nothing installed: the binary was
        // removed by hand, and the command line builds it again.
        None if state.standing == PinStanding::Missing => format!(
            "{product} was built from source here before, so it is built again from a terminal: \
             run '{command}'."
        ),
        None => format!(
            "{product} was built from source here, so it is updated from a terminal, which \
             builds it again: run '{command}'."
        ),
    }))
}

/// A conflict while a model runs on `kind`'s server: replacing or removing
/// the binary and its library under a running server leaves it serving from
/// deleted files and the next launch failing. `doing` is what was refused,
/// `installing` or `removing`.
pub(crate) async fn refuse_while_serving(
    runtime: &dyn ModelRuntimePort,
    kind: RuntimeKind,
    doing: &str,
) -> Result<(), GuiError> {
    running_model(runtime, kind).await.map_or(Ok(()), |model| {
        Err(GuiError::Conflict(format!(
            "{model} is running on {}. Stop it before {doing} {}.",
            kind.server_name(),
            kind.label()
        )))
    })
}
