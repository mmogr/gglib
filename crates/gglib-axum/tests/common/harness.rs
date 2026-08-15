//! Bootstrap harness for the integration tests in this crate.
//!
//! Every test here needs the same two properties, and wrote them out by hand:
//!
//! 1. **Isolation.** [`bootstrap`] without a `db_path` resolves through
//!    `gglib_core::paths::database_path()`, which in a debug build is the
//!    developer's own checkout. No suite had this. Tests were reading rows
//!    they did not create and leaving rows behind for the next run.
//! 2. **Honesty.** A bootstrap failure has to fail the test. Every suite but
//!    `daemon_route_contract` skipped instead — `Err(_) => return` in three
//!    of them, `bootstrap(..).ok()?` behind a let-else in `daemon_access` —
//!    which turns a contract test into a no-op that reports success.
//!
//! Routing every suite through [`test_state`] leaves one place to get both
//! right. Nothing enforces that: `gglib_axum::bootstrap` is `pub` and no CI
//! check covers this directory, so a new test can still open the developer's
//! database by writing the four lines out again.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, LazyLock};

use axum::Router;
use gglib_axum::{AxumContext, DaemonAccess, ServerConfig, bootstrap, create_router};
use gglib_core::CorsConfig;

use super::ports::TEST_BASE_PORT;

/// Scratch directory for this test process.
///
/// Under Cargo's own scratch directory so `cargo clean` collects it, and
/// keyed by both the executable's file name and the process id.
///
/// The pid is what makes it correct, not decoration. An earlier version
/// keyed on the binary alone and emptied the directory on first use, which
/// two concurrent `cargo test` runs of the same binary defeat exactly: both
/// wipe the shared directory, both restart the counter at zero, and both
/// open `gglib-0.db` — reproduced as `SQLITE_BUSY: database is locked`. A
/// per-process directory cannot be shared, so nothing needs emptying and
/// there is no destructive step left to order against anything.
///
/// (Cargo runs this crate's test binaries one at a time; it is the tests
/// *within* a binary that share a process. The binary name is in the path
/// for legibility, not for exclusion.)
static SCRATCH_DIR: LazyLock<PathBuf> = LazyLock::new(|| {
    let exe = std::env::current_exe().expect("locate the test executable");
    let binary = exe
        .file_name()
        .expect("test executable has a file name")
        .to_string_lossy();

    let dir =
        PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("{binary}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("create isolated data dir");
    dir
});

/// A database of its own for each context.
///
/// Per test rather than per binary: tests in one binary run concurrently, so
/// a shared file both deadlocks on SQLite's write lock and lets one test see
/// another's rows.
fn isolated_db_path() -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    SCRATCH_DIR.join(format!("gglib-{}.db", NEXT.fetch_add(1, Ordering::Relaxed)))
}

/// Config for a context that binds nothing and launches nothing.
fn test_config(cors: CorsConfig) -> ServerConfig {
    ServerConfig {
        host: "127.0.0.1".into(),
        port: 0,
        base_port: TEST_BASE_PORT,
        llama_server_path: "/nonexistent/llama-server".into(),
        max_concurrent_agent_loops: 1,
        static_dir: None,
        cors,
        db_path: Some(isolated_db_path()),
    }
}

/// Bootstrap a context against a database of its own.
///
/// # Panics
///
/// If bootstrap fails. That is the point, and it is #842's convention: a
/// route-contract test that passes silently is the failure mode it exists to
/// prevent, since #834 shipped a deleted route through a fully green suite.
pub(crate) async fn test_state(cors: CorsConfig) -> Arc<AxumContext> {
    let ctx = bootstrap(test_config(cors))
        .await
        .expect("bootstrap an isolated test context");

    Arc::new(ctx)
}

/// The access policy most tests here run under: loopback, no token.
#[allow(dead_code)] // each test binary uses a different subset
pub(crate) fn test_access() -> Arc<DaemonAccess> {
    Arc::new(DaemonAccess::loopback())
}

/// [`test_state`] plus the router over it, for tests that assert on both.
#[allow(dead_code)] // each test binary uses a different subset
pub(crate) async fn test_state_and_app(cors: CorsConfig) -> (Arc<AxumContext>, Router) {
    let state = test_state(cors.clone()).await;
    let router = create_router(Arc::clone(&state), &cors, test_access());
    (state, router)
}

/// [`test_state_and_app`] for the tests that never touch the context.
#[allow(dead_code)] // each test binary uses a different subset
pub(crate) async fn test_app(cors: CorsConfig) -> Router {
    test_state_and_app(cors).await.1
}

/// [`test_app`] under an access policy other than plain loopback.
#[allow(dead_code)] // each test binary uses a different subset
pub(crate) async fn test_app_with_access(cors: CorsConfig, access: DaemonAccess) -> Router {
    let state = test_state(cors.clone()).await;
    create_router(state, &cors, Arc::new(access))
}
