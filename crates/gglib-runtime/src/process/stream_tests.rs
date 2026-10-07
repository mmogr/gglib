//! Tests for the log readers: which lines the log manager is handed, and
//! under which port.
//!
//! The log manager is one per process, so each test reads a port no other
//! test writes to.

use std::time::Duration;

use super::*;
use crate::process::ServerLogEntry;

/// The lines the log manager holds for `port`, once there are `n` of them or
/// ten seconds have passed.
async fn lines(port: u16, n: usize) -> Vec<ServerLogEntry> {
    let _ = tokio::time::timeout(Duration::from_secs(10), async {
        while get_log_manager().get_logs(port).len() < n {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await;
    get_log_manager().get_logs(port)
}

fn text(entries: &[ServerLogEntry]) -> Vec<&str> {
    entries.iter().map(|e| e.line.as_str()).collect()
}

#[tokio::test]
async fn each_line_of_a_stream_reaches_the_log_manager_under_the_readers_port() {
    const PORT: u16 = 61_001;
    spawn_stream_reader(
        std::io::Cursor::new(b"first\nsecond\r\nlast, unterminated".to_vec()),
        PORT,
        "stdout",
    );

    let held = lines(PORT, 3).await;

    assert_eq!(text(&held), ["first", "second", "last, unterminated"]);
    assert!(held.iter().all(|e| e.port == PORT), "{held:?}");
}

/// llama-server writes bytes that are not UTF-8; the line they are on is
/// kept, lossily, and so is every line after it.
#[tokio::test]
async fn bytes_that_are_not_utf8_do_not_end_the_reader() {
    const PORT: u16 = 61_002;
    spawn_stream_reader(
        std::io::Cursor::new(b"bad \xff byte\nstill reading\n".to_vec()),
        PORT,
        "stderr",
    );

    let held = lines(PORT, 2).await;

    assert_eq!(text(&held), ["bad \u{fffd} byte", "still reading"]);
}

/// Two servers spawned by one core, each writing a line to both of its
/// streams: every line is under the port of the server that wrote it.
///
/// llama-server here is a script that prints its arguments, which include
/// the port it was launched on, and exits. Unix-only for that script.
#[cfg(unix)]
#[tokio::test]
async fn a_spawned_servers_stdout_and_stderr_reach_the_log_manager_under_its_port() {
    use std::os::unix::fs::PermissionsExt;

    use crate::pidfile::delete_pidfile;
    use crate::process::GuiProcessCore;
    use gglib_core::ports::ServerConfig;

    // Ids and a base port no other test in this binary uses.
    const IDS: [i64; 2] = [999_016, 999_017];
    const BASE_PORT: u16 = 19_350;

    gglib_core::paths::isolate_data_root();
    let dir = tempfile::tempdir().expect("temp dir");
    let script = dir.path().join("llama-server");
    std::fs::write(&script, "#!/bin/sh\necho \"out $*\"\necho \"err $*\" >&2\n")
        .expect("write script");
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    let model = dir.path().join("model.gguf");
    std::fs::write(&model, b"not really a gguf").expect("write model file");
    let mut core = GuiProcessCore::new(BASE_PORT, script.to_string_lossy());

    let mut ports = Vec::new();
    for id in IDS {
        let config = ServerConfig::new(id, "test-model".to_owned(), model.clone(), BASE_PORT);
        let (port, _pid) = core.spawn(config).await.expect("spawn");
        ports.push(port);
    }
    let held = [lines(ports[0], 2).await, lines(ports[1], 2).await];
    for id in IDS {
        delete_pidfile(id).ok();
    }

    assert_ne!(ports[0], ports[1], "each server has a port of its own");
    for (port, held) in ports.iter().zip(&held) {
        let mut seen = text(held);
        seen.sort_unstable();
        let launched_on = format!(" --port {port} ");
        assert_eq!(seen.len(), 2, "one line from each stream: {seen:?}");
        assert!(
            seen[0].starts_with("err ") && seen[0].contains(&launched_on),
            "{seen:?}"
        );
        assert!(
            seen[1].starts_with("out ") && seen[1].contains(&launched_on),
            "{seen:?}"
        );
        assert!(held.iter().all(|e| e.port == *port), "{held:?}");
    }
}
