//! `--max-tool-iterations` above the ceiling, through the binary: `chat`
//! and `q` each say on stderr, in one line, the value asked and the ceiling
//! the run takes instead, and stdout is what it would have been.
//!
//! Each command names a model no library holds, so it ends at the lookup,
//! after the flag is read and before any daemon or port is involved. The
//! limit a run takes is tested beside `step_limit`.

use std::net::TcpListener;
use std::path::Path;
use std::process::Output;

use gglib_core::MAX_ITERATIONS_CEILING;
use gglib_core::utils::process::cmd;

/// An identifier no library holds.
const MISSING: &str = "__no_such_model__";

/// A loopback port nothing listens on, for `GGLIB_DAEMON_PORT`: the
/// command never reaches a daemon, and must not reach the owner's.
fn idle_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
    listener.local_addr().expect("its address").port()
}

/// `gglib` run with `args` over the data folder `root`.
fn gglib(root: &Path, args: &[&str]) -> Output {
    cmd(env!("CARGO_BIN_EXE_gglib"))
        .args(args)
        .env("GGLIB_DATA_DIR", root)
        .env("GGLIB_DAEMON_PORT", idle_port().to_string())
        .env("NO_COLOR", "1")
        .output()
        .unwrap_or_else(|e| panic!("running `gglib {}`: {e}", args.join(" ")))
}

/// Run as `chat` and as `q`, a limit above the ceiling adds one line to
/// stderr, naming both numbers, and leaves stdout and the exit code as a
/// limit at the ceiling leaves them.
#[test]
fn a_limit_above_the_ceiling_says_so_on_stderr_alone() {
    let root = tempfile::tempdir().expect("a temporary directory");
    let (ceiling, above) = (
        MAX_ITERATIONS_CEILING.to_string(),
        (MAX_ITERATIONS_CEILING + 10).to_string(),
    );
    let notice = format!(
        "--max-tool-iterations {above} is more than the {ceiling} steps a run may take; \
         it runs with {ceiling}.\n"
    );
    for command in [&["chat", MISSING][..], &["q", "hi", "--model", MISSING][..]] {
        let label = command.join(" ");
        let with = |limit: &str| {
            let mut args = command.to_vec();
            args.extend(["--max-tool-iterations", limit]);
            gglib(root.path(), &args)
        };
        let (at, over) = (with(&ceiling), with(&above));
        let (at_err, over_err) = (
            String::from_utf8_lossy(&at.stderr),
            String::from_utf8_lossy(&over.stderr),
        );

        assert!(
            !at_err.contains("--max-tool-iterations"),
            "`gglib {label}` at the ceiling says nothing of it: {at_err}"
        );
        assert_eq!(
            over_err,
            format!("{notice}{at_err}"),
            "`gglib {label}` above the ceiling says so first, and nothing else changes"
        );
        assert_eq!(
            over.stdout, at.stdout,
            "`gglib {label}`: stdout is unchanged"
        );
        assert_eq!(over.status.code(), at.status.code(), "`gglib {label}`");
    }
}
