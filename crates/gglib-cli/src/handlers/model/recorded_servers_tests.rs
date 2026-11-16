//! What the view does not count, and what it will not do.
//!
//! No llama-server is installed under this binary's data root, so no
//! process here can be one. A record that counts, and the ways one stops
//! counting, are tested against the built binary, with a data root and a
//! stand-in server of the test's own, in `tests/model_served.rs`.

use gglib_core::ports::ModelRuntimePort;
use gglib_runtime::pidfile::{delete_pidfile, list_pidfiles, write_pidfile};

use super::RecordedServers;

/// An id no model in this binary's libraries has: each test's library is in
/// a directory of its own, but the pid files are in the one data root the
/// binary's tests share.
const RECORDED_ID: i64 = 999_301;

/// A pid file alone is not a server: its pid has to be a llama-server of
/// ours. This test's own process is running, and is not one.
#[tokio::test]
async fn a_record_whose_pid_is_not_our_llama_server_is_not_listed() {
    gglib_core::paths::isolate_data_root();
    write_pidfile(RECORDED_ID, std::process::id(), 9001).expect("the record is written");
    let recorded = list_pidfiles().expect("the records read");
    let listed = RecordedServers.list_running().await;
    delete_pidfile(RECORDED_ID).expect("the record is removed");

    assert!(
        recorded.iter().any(|(id, _)| *id == RECORDED_ID),
        "the record was not there to be read"
    );
    assert!(
        !listed.iter().any(|server| server.model_id == RECORDED_ID),
        "{listed:?}"
    );
}

/// `ModelOps::remove` with the request's `force` stops the server and then
/// removes the model. Told "stopped" by a view that stops nothing, it would
/// remove a model that is still being served.
#[tokio::test]
async fn the_view_does_not_say_it_stopped_a_server() {
    assert!(RecordedServers.stop_current().await.is_err());
}

/// The issue's check (#2025): a one-shot plan's program is the engine where
/// `gglib config paths` says gglib keeps it, not a bare `llama-server`,
/// which nobody's PATH holds. Nothing is installed under this binary's
/// data root, so the plan also says so, in the words a load would fail
/// with.
#[test]
fn a_one_shot_plan_names_the_engine_where_gglib_keeps_it() {
    gglib_core::paths::isolate_data_root();
    let engine = gglib_core::paths::llama_server_path().expect("the data root resolves");
    let plan = RecordedServers
        .plan(
            &gglib_core::ports::ModelLaunchSpec {
                id: 1,
                name: "qwen".to_owned(),
                file_path: "/models/qwen.gguf".into(),
                projector: None,
                image_family: None,
                components: Vec::new(),
                tags: Vec::new(),
                architecture: None,
                quantization: None,
                context_length: None,
                server_defaults: None,
                file_size_bytes: 0,
                kv_shape: None,
                kv_memory_is_partial: false,
                model_sampling: gglib_core::domain::ModelSamplingDefaults::default(),
            },
            None,
            None,
            &gglib_core::ports::LaunchOverrides::default(),
        )
        .expect("a chat model plans");

    assert_eq!(plan.program, engine.to_string_lossy());
    let said = plan.not_installed.expect("nothing is installed here");
    assert!(said.contains(&*engine.to_string_lossy()), "{said}");
}
