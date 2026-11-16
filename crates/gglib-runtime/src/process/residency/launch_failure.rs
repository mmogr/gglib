//! Why a launch failed, read from what failed: the spawn's error, the health
//! wait's, or how the server ended and what it printed (#1959 C5). Each
//! answer is a [`LoadCause`], which `gglib_core::ports::LoadFailure` words.

use std::path::Path;

use gglib_core::domain::RuntimeKind;
use gglib_core::ports::LoadCause;
use gglib_core::utils::process::bounded::{Ended, Failure, failure_kind};

use crate::engine::NotWorking;
use crate::process::core::Exit;
use crate::process::health::{NotReadyInTime, PortHeld};
use crate::process::output_tail::{OutputTail, SETTLE_WAIT, telling_line};

/// Why `runtime`'s server, from `binary`, was not started (`e`, the process
/// core's error). A binary that is not there, or that the system would not
/// start, is the engine not working, in the words its check uses, so the
/// same failure reads the same whether or not a status asked first; any
/// other reason is the core's.
pub(in crate::process::residency) fn not_started(
    e: &anyhow::Error,
    runtime: RuntimeKind,
    binary: &Path,
) -> LoadCause {
    let failure = if binary.is_file() {
        e.chain()
            .find_map(|cause| cause.downcast_ref::<std::io::Error>())
            .map(|io| failure_kind(Ended::NotStarted(io)))
    } else {
        Some(Failure::NotFound)
    };
    failure.map_or_else(
        || LoadCause::NotStarted {
            reason: format!("{e:#}"),
        },
        |failure| LoadCause::EngineNotWorking {
            reason: NotWorking::when_started(runtime, binary, failure.clone()).to_string(),
            failure,
        },
    )
}

/// Why a server never answered as ready (`e`, the health wait's error):
/// not ready in time, with the line it printed; or another program on its
/// port. `None` when the wait itself could not be made.
pub(in crate::process::residency) fn not_healthy(
    e: &anyhow::Error,
    output: &OutputTail,
) -> Option<LoadCause> {
    if let Some(late) = e.downcast_ref::<NotReadyInTime>() {
        return Some(LoadCause::NotReady {
            waited_secs: late.timeout_secs,
            said: output.said(),
        });
    }
    e.downcast_ref::<PortHeld>()
        .map(|held| LoadCause::PortTaken { port: held.port })
}

/// Why a server that ended while it loaded did: how it ended, read by
/// [`failure_kind`] from its status and all it printed (where each OS's
/// loader says which library it could not load), and the line that says
/// why. An exit with no status to read, or a 0, is no failure.
///
/// Its output is read once its readers have reached the end of it (or
/// [`SETTLE_WAIT`] has passed): a server that printed why and exited at
/// once is seen to exit before the last of its lines are read.
pub(in crate::process::residency) async fn stopped(exit: Exit, output: &OutputTail) -> LoadCause {
    output.settled(SETTLE_WAIT).await;
    let lines = output.lines();
    let failure = match exit {
        Exit::Status(status) if !status.success() => Some(failure_kind(Ended::Exited {
            code: status.code(),
            stderr: &lines.join("\n"),
        })),
        Exit::Status(_) | Exit::Unknown => None,
    };
    LoadCause::Stopped {
        failure,
        said: telling_line(&lines),
    }
}
