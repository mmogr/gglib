//! The far list as a person reads it: one row per model by its id there,
//! its variants' profiles folded into that row, and how to send it a turn.

use std::sync::Mutex;

use gglib_app_services::RemoteConnection;
use gglib_core::Settings;
use gglib_proxy::models::ModelInfo;

use super::{ConnectionReport, footer, heading, render, rows, summary, summary_from, summary_line};
use crate::bootstrap::{CliContext, test_context};

/// Entries as the far proxy lists them, `(id, gglib_id, profile, context)`.
fn listed(entries: &[(&str, i64, Option<&str>, Option<u64>)]) -> Vec<ModelInfo> {
    entries
        .iter()
        .map(|(id, gglib_id, profile, context)| {
            serde_json::from_value(serde_json::json!({
                "id": id,
                "gglib_id": gglib_id,
                "profile": profile,
                "object": "model",
                "created": 1,
                "owned_by": "gglib",
                "context_window": context,
            }))
            .unwrap()
        })
        .collect()
}

#[test]
fn a_models_variants_are_its_profiles_not_rows_of_their_own() {
    let models = listed(&[
        ("qwen3", 3, None, Some(30_000)),
        ("qwen3:coding", 3, Some("coding"), Some(30_000)),
        ("qwen3:fast", 3, Some("fast"), None),
        ("llama", 7, None, None),
    ]);

    let rows = rows(&models);
    let table = render(&rows);

    assert_eq!(rows.len(), 2, "one row per id: {table}");
    let lines: Vec<&str> = table.lines().collect();
    assert_eq!(
        lines[0].split_whitespace().collect::<Vec<_>>(),
        ["ID", "NAME", "CONTEXT", "PROFILES"]
    );
    assert_eq!(
        lines[2].split_whitespace().collect::<Vec<_>>(),
        ["3", "qwen3", "30000", "coding,", "fast"]
    );
    assert!(lines[2].ends_with("coding, fast"), "{table}");
    assert_eq!(
        lines[3].split_whitespace().collect::<Vec<_>>(),
        ["7", "llama", "-", "-"],
        "no context and no profiles are both a '-'"
    );
}

/// The ID column is as wide as the widest id, so a four-digit id does not
/// push its row's name out of line with the rest.
#[test]
fn the_id_column_is_as_wide_as_the_widest_id() {
    let models = listed(&[("small", 3, None, None), ("big", 1000, None, None)]);

    let table = render(&rows(&models));

    let lines: Vec<&str> = table.lines().collect();
    assert_eq!(lines[0].find("NAME"), lines[2].find("small"), "{table}");
    assert_eq!(lines[2].find("small"), lines[3].find("big"), "{table}");
    assert!(lines[2].starts_with("   3  "), "{table}");
    assert!(lines[3].starts_with("1000  "), "{table}");
}

/// A variant listed without its base — a pinned endpoint can list only what
/// it serves — still names its model, by the name before the profile.
#[test]
fn a_variant_without_its_base_names_the_model_it_is_of() {
    let models = listed(&[("org/qwen3:coding", 4, Some("coding"), None)]);

    let rows = rows(&models);

    assert_eq!(rows[0].name, "org/qwen3");
    assert_eq!(rows[0].profiles, ["coding"]);
}

#[test]
fn the_footer_says_how_to_chat_with_one_by_id() {
    let plain = listed(&[("qwen3", 3, None, None)]);
    let profiled = listed(&[
        ("qwen3", 3, None, None),
        ("qwen3:coding", 3, Some("coding"), None),
    ]);

    let without = footer(&rows(&plain));
    let with = footer(&rows(&profiled));

    assert!(
        without.contains("Chat with one: gglib chat <id> --remote"),
        "{without}"
    );
    assert!(!without.contains("<profile>"), "{without}");
    assert!(
        with.contains("gglib chat <id>:<profile> --remote"),
        "{with}"
    );
}

/// The list is headed by the name the paired machine is shown by, with or
/// without models on it; its fingerprint is identity and never printed.
#[test]
fn the_heading_names_the_machine_and_no_fingerprint() {
    assert_eq!(
        heading(2, "desk", "direct"),
        "2 model(s) on desk (direct):\n\n"
    );
    assert_eq!(heading(0, "desk", "direct"), "No models on desk.\n");
    assert_eq!(
        heading(0, gglib_core::domain::UNNAMED_PAIRED, "relayed"),
        "No models on the paired machine.\n"
    );
}

// ── The line `gglib model list` ends with ────────────────────────────────

/// The daemon's report of the connection to the paired machine.
fn connection(path: &str, away_for_s: Option<u64>) -> RemoteConnection {
    RemoteConnection {
        port: 41234,
        base_url: "http://127.0.0.1:41234/v1".to_owned(),
        ticket_fingerprint: "0123456789ab".to_owned(),
        path: path.to_owned(),
        away_for_s,
    }
}

/// A daemon as [`summary`] sees it: running or not, reporting
/// `connection`, and recording every question it is asked. It can answer
/// nothing else, so a question to the paired machine would not compile.
struct FakeDaemon {
    running: bool,
    connection: Option<RemoteConnection>,
    asked: Mutex<Vec<&'static str>>,
}

impl FakeDaemon {
    fn new(running: bool, connection: Option<RemoteConnection>) -> Self {
        Self {
            running,
            connection,
            asked: Mutex::new(Vec::new()),
        }
    }

    fn asked(&self) -> Vec<&'static str> {
        self.asked.lock().unwrap().clone()
    }
}

impl ConnectionReport for FakeDaemon {
    async fn running(&self) -> bool {
        self.asked.lock().unwrap().push("probe");
        self.running
    }

    async fn connection(&self) -> Option<RemoteConnection> {
        self.asked.lock().unwrap().push("status");
        self.connection.clone()
    }
}

/// The CLI's context over `dir`'s database, paired with a machine called
/// `desk` when `paired`.
async fn context(dir: &tempfile::TempDir, paired: bool) -> CliContext {
    let ctx = test_context(dir.path()).await;
    if paired {
        ctx.settings_repo
            .modify(&|settings: &mut Settings| {
                settings.remote_pairing = Some(gglib_core::RemotePairing {
                    ticket: "not-a-ticket".to_owned(),
                    api_key: "key".to_owned(),
                    default_model: None,
                    port: None,
                    name: Some("desk".to_owned()),
                });
                Ok(())
            })
            .await
            .expect("paired");
    }
    ctx
}

/// An unpaired machine's list ends with its own table: nothing is said,
/// and no daemon is asked.
#[tokio::test]
async fn an_unpaired_machine_says_nothing_of_another() {
    let dir = tempfile::tempdir().expect("tempdir");
    let ctx = context(&dir, false).await;
    let daemon = FakeDaemon::new(true, Some(connection("direct", None)));

    assert_eq!(summary(&ctx).await, None);
    assert_eq!(summary_from(&ctx, &daemon).await, None);
    assert!(daemon.asked().is_empty(), "{:?}", daemon.asked());
}

/// A daemon that is down: the pairing is named as not connected, and the
/// daemon is asked nothing past its probe.
#[tokio::test]
async fn a_paired_machine_with_the_daemon_down_is_not_connected() {
    let dir = tempfile::tempdir().expect("tempdir");
    let ctx = context(&dir, true).await;
    let daemon = FakeDaemon::new(false, None);

    let line = summary_from(&ctx, &daemon).await;

    assert_eq!(line.as_deref(), Some("desk: not connected"));
    assert_eq!(daemon.asked(), ["probe"]);
}

/// Connected, and away: said from the daemon's status alone, the one
/// question past its probe. Nothing is asked of the paired machine, so
/// there is nothing to wait on while that machine is gone.
#[tokio::test]
async fn a_connected_or_away_machine_is_said_from_the_daemons_status() {
    let dir = tempfile::tempdir().expect("tempdir");
    let ctx = context(&dir, true).await;
    let connected = FakeDaemon::new(true, Some(connection("direct", None)));
    let away = FakeDaemon::new(true, Some(connection("relayed", Some(125))));

    assert_eq!(
        summary_from(&ctx, &connected).await.as_deref(),
        Some("Paired with desk (direct). Its models: gglib model list --remote")
    );
    assert_eq!(
        summary_from(&ctx, &away).await.as_deref(),
        Some("desk: away 2m")
    );
    assert_eq!(connected.asked(), ["probe", "status"]);
    assert_eq!(away.asked(), ["probe", "status"]);
}

/// Connected: the machine by its name, how it is reached, and the one
/// command that lists its models. The command stands alone in its sentence.
#[test]
fn a_connected_machine_is_named_with_the_command_that_lists_it() {
    let line = summary_line(Some("desk"), Some(&connection("direct", None)));

    assert_eq!(
        line,
        "Paired with desk (direct). Its models: gglib model list --remote"
    );
    assert!(!line.contains(';'), "never two commands joined: {line}");
}

/// A daemon up and not connected reports no connection: the pairing is
/// still named, so the list says whose models are missing.
#[test]
fn a_machine_with_no_connection_is_named_as_not_connected() {
    assert_eq!(summary_line(Some("desk"), None), "desk: not connected");
    assert_eq!(
        summary_line(None, None),
        "the paired machine: not connected",
        "a machine that gave no name is called what every surface calls it"
    );
}
