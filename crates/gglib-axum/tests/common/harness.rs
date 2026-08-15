//! Bootstrap harness for the integration tests in this crate.
//!
//! Every test here needs the same two properties, and got neither by writing
//! them out by hand:
//!
//! 1. **Isolation.** [`bootstrap`] without a `db_path` resolves through
//!    `gglib_core::paths::database_path()`, which in a debug build is the
//!    developer's own checkout. Tests were reading rows they did not create
//!    and leaving rows behind for the next run.
//! 2. **Honesty.** A bootstrap failure has to fail the test. The hand-written
//!    form was `Err(_) => return`, which turns a contract test into a no-op
//!    that reports success — the failure mode a contract test exists to catch.
//!
//! [`test_state`] is the only way to build a context here, so both properties
//! hold by construction rather than by everyone remembering.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, LazyLock};

use axum::Router;
use gglib_axum::{AxumContext, DaemonAccess, ServerConfig, bootstrap, create_router};
use gglib_core::CorsConfig;

use super::ports::TEST_BASE_PORT;

/// Scratch directory for this test binary, emptied once per run.
///
/// Under Cargo's own scratch directory so `cargo clean` collects it, and
/// keyed by the running executable's file name because Cargo runs this
/// crate's test binaries concurrently. `LazyLock` is what makes the wipe
/// safe: it runs the initialiser exactly once, with every other test
/// blocked until it finishes, so no test can be emptying the directory
/// while a sibling holds a database open inside it.
static SCRATCH_DIR: LazyLock<PathBuf> = LazyLock::new(|| {
    let binary = std::env::current_exe()
        .ok()
        .and_then(|path| path.file_name().map(std::ffi::OsStr::to_os_string))
        .expect("test executable has a file name");

    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(binary);
    let _ = std::fs::remove_dir_all(&dir);
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
/// If bootstrap fails. That is the point: a test that skips itself when its
/// fixture will not build reports success for a surface it never reached.
#[allow(dead_code)] // each test binary uses a different subset
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
