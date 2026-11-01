//! `gglib mcp add`, `list`, `tools`, `test`, `enable`, `disable` and
//! `remove`, run as a person runs them against a database on disk, and what
//! `gglib chat` and `gglib q` say of a stored server they cannot run.
//!
//! The unit tests hand the service a repository they built; these run the
//! binary, so the tables are the ones its own bootstrap creates under
//! `GGLIB_DATA_DIR`. Nothing here reaches a network: the one server that is
//! started is a shell script in the test's own directory, the chat commands
//! are pointed at a port nothing listens on, and a `gglib mcp` command's
//! daemon is a port nothing listens on or a stand-in in this process.

use std::io::Write as _;
use std::net::TcpListener;
use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::sync::{Arc, Mutex};

use gglib_app_services::types::{
    McpServerConfigDto, McpServerDto, McpServerInfo, McpServerStatusDto, McpToolInfo,
};
use gglib_core::contracts::http::daemon::{MCP_SERVERS_PATH, mcp_server_start_path};
use gglib_core::domain::mcp::{McpLifecycle, NewMcpServer};
use gglib_test_http::blocking;

#[path = "support/data_dir.rs"]
mod data_dir;

/// What an SSE server is refused with, wherever it is refused.
const SSE_NOT_SUPPORTED: &str = "SSE servers are not supported yet; only stdio servers can be run";

/// Store an SSE server in `root`'s database, as one added while they were
/// accepted is stored: `gglib mcp add` refuses one now.
fn stored_sse(root: &Path, name: &str, lifecycle: McpLifecycle) {
    let server = NewMcpServer::new_sse(name, "http://localhost:3001/sse").with_lifecycle(lifecycle);
    data_dir::store_mcp_server(root, server);
}

/// A loopback port nothing listens on.
fn closed_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
    listener.local_addr().expect("its address").port()
}

/// What `gglib <args>` printed, stdout then stderr, over the data directory
/// `root`: at the log level a run has when nothing sets one, with nothing on
/// stdin, and no daemon (its port is one nothing listens on).
fn printed(root: &Path, args: &[&str]) -> (String, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_gglib"))
        .args(args)
        .env("GGLIB_DATA_DIR", root)
        .env("GGLIB_DAEMON_PORT", closed_port().to_string())
        .env_remove("RUST_LOG")
        .stdin(Stdio::null())
        .output()
        .unwrap_or_else(|e| panic!("running `gglib {}`: {e}", args.join(" ")));
    let text = |bytes: &[u8]| String::from_utf8_lossy(bytes).into_owned();
    (text(&out.stdout), text(&out.stderr))
}

/// `gglib mcp <args>` over the data directory `root`, with no daemon: its
/// port is one nothing listens on.
fn mcp(root: &Path, args: &[&str]) -> Output {
    mcp_on(root, closed_port(), args)
}

/// `gglib mcp <args>` over the data directory `root`, with the daemon on
/// `port`.
fn mcp_on(root: &Path, port: u16, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_gglib"))
        .arg("mcp")
        .args(args)
        .env("GGLIB_DATA_DIR", root)
        .env("GGLIB_DAEMON_PORT", port.to_string())
        .output()
        .unwrap_or_else(|e| panic!("running `gglib mcp {}`: {e}", args.join(" ")))
}

/// What `out` printed on stdout, once it has succeeded.
fn succeeded(out: &Output) -> String {
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    assert!(
        out.status.success(),
        "must succeed\nstdout: {stdout}\nstderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    stdout
}

/// What `gglib mcp <args>` printed, once it has succeeded.
fn mcp_ok(root: &Path, args: &[&str]) -> String {
    let out = mcp(root, args);
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    assert!(
        out.status.success(),
        "`gglib mcp {}` must succeed\nstdout: {stdout}\nstderr: {}",
        args.join(" "),
        String::from_utf8_lossy(&out.stderr)
    );
    stdout
}

/// What `gglib mcp <args>` wrote to stderr, once it has failed.
fn mcp_err(root: &Path, args: &[&str]) -> String {
    let out = mcp(root, args);
    assert!(
        !out.status.success(),
        "`gglib mcp {}` must fail\nstdout: {}",
        args.join(" "),
        String::from_utf8_lossy(&out.stdout)
    );
    String::from_utf8_lossy(&out.stderr).into_owned()
}

/// An MCP server for `sh <file>`: it answers `initialize` and `tools/list`
/// with the id it was asked under, and offers one tool.
#[cfg(unix)]
const STAND_IN_SERVER: &str = r#"while IFS= read -r line; do
  id=${line#*\"id\":}
  id=${id%%,*}
  case "$line" in
    *'"method":"initialize"'*)
      printf '%s\n' "{\"jsonrpc\":\"2.0\",\"id\":$id,\"result\":{\"protocolVersion\":\"2024-11-05\",\"capabilities\":{\"tools\":{}},\"serverInfo\":{\"name\":\"stand-in\"}}}" ;;
    *'"method":"tools/list"'*)
      printf '%s\n' "{\"jsonrpc\":\"2.0\",\"id\":$id,\"result\":{\"tools\":[{\"name\":\"echo\",\"description\":\"says it back\"}]}}" ;;
  esac
done
"#;

/// With a URL and without one: the type is what is refused, so a missing URL
/// is not asked for first.
#[test]
fn adding_an_sse_server_is_refused_with_the_reason_and_stores_nothing() {
    let root = tempfile::tempdir().expect("temp data dir");
    let add = ["add", "--name", "remote", "--type", "sse"];

    for url in [&["--url", "http://localhost:3001/sse"][..], &[]] {
        let refused = mcp_err(root.path(), &[&add[..], url].concat());

        assert!(
            refused.contains(&format!("Error: {SSE_NOT_SUPPORTED}")),
            "got: {refused}"
        );
    }

    let listed = mcp_ok(root.path(), &["list"]);
    assert!(
        listed.contains("No MCP servers configured."),
        "got: {listed}"
    );
}

#[test]
fn the_help_for_add_says_of_sse_and_its_url_that_they_are_not_supported_yet() {
    let root = tempfile::tempdir().expect("temp data dir");

    let help = mcp_ok(root.path(), &["add", "--help"]);

    let said_of = |flag: &str| {
        help.lines()
            .find(|line| line.trim_start().starts_with(flag))
            .unwrap_or_else(|| panic!("no line for {flag}, got: {help}"))
    };
    assert!(
        said_of("--type").contains(r#""sse" (HTTP) is not supported yet"#),
        "got: {help}"
    );
    assert!(
        said_of("--url").contains("not supported yet"),
        "got: {help}"
    );
}

#[test]
fn a_stored_sse_server_is_listed_as_not_supported_yet_refused_a_run_and_an_edit_and_still_removed()
{
    let root = tempfile::tempdir().expect("temp data dir");
    stored_sse(root.path(), "remote", McpLifecycle::Lazy);

    let listed = mcp_ok(root.path(), &["list"]);
    let row = listed
        .lines()
        .find(|line| line.contains("remote"))
        .unwrap_or_else(|| panic!("no row for the server, got: {listed}"));
    assert!(
        row.contains(" sse ") && row.ends_with("not supported yet"),
        "the row names its type and says it is not supported, got: {row}"
    );

    for command in ["tools", "test", "enable", "disable"] {
        let refused = mcp_err(root.path(), &[command, "remote"]);

        assert!(
            refused.contains(&format!("Error: {SSE_NOT_SUPPORTED}")),
            "`mcp {command}` got: {refused}"
        );
    }

    let removed = mcp_ok(root.path(), &["remove", "remote", "--yes"]);
    assert!(
        removed.contains("Removed MCP server 'remote'"),
        "got: {removed}"
    );
    let listed = mcp_ok(root.path(), &["list"]);
    assert!(
        listed.contains("No MCP servers configured."),
        "got: {listed}"
    );
}

/// Both commands start the eager servers and then the lazy ones before the
/// first turn, with tools switched off as with them on. Neither is to try a
/// server it cannot run, so neither has a failure to warn of.
///
/// Each run is seen to get past that start: the chat session opens, and
/// `gglib q` goes on to fail at the port nothing listens on.
#[test]
fn chat_commands_say_nothing_of_a_stored_sse_server() {
    let root = tempfile::tempdir().expect("temp data dir");
    stored_sse(root.path(), "remote-lazy", McpLifecycle::Lazy);
    stored_sse(root.path(), "remote-eager", McpLifecycle::Eager);
    let chat = (&["chat", "qwen"][..], "Agentic chat ready");
    let question = (
        &["q", "hi", "--model", "qwen"][..],
        "request to llama-server failed",
    );

    for (command, past_the_start) in [chat, question] {
        for tools in [&[][..], &["--no-tools"]] {
            let port = closed_port().to_string();
            let args = [command, &["--port", &port], tools].concat();

            let (stdout, stderr) = printed(root.path(), &args);

            let ran = format!(
                "`gglib {}`\nstdout: {stdout}\nstderr: {stderr}",
                args.join(" ")
            );
            assert!(
                stdout.contains(past_the_start) || stderr.contains(past_the_start),
                "{ran}"
            );
            assert!(!stderr.contains("MCP"), "{ran}");
        }
    }
}

#[test]
fn adding_a_server_under_a_taken_name_is_refused_by_name() {
    let root = tempfile::tempdir().expect("temp data dir");
    let add = ["add", "--name", "files", "--type", "stdio", "--command"];
    mcp_ok(root.path(), &[&add[..], &["one"]].concat());

    let refused = mcp_err(root.path(), &[&add[..], &["two"]].concat());

    assert!(
        refused.contains("An MCP server named 'files' already exists; choose another name"),
        "got: {refused}"
    );
    let listed = mcp_ok(root.path(), &["list"]);
    assert!(listed.contains("Found 1 MCP server(s)"), "got: {listed}");
}

/// The second `TOKEN` breaks the env table's one-row-per-key rule after the
/// server row has been written, so the add fails part-way through.
#[test]
fn an_add_that_fails_on_its_env_leaves_no_server() {
    let root = tempfile::tempdir().expect("temp data dir");

    mcp_err(
        root.path(),
        &[
            "add",
            "--name",
            "half",
            "--type",
            "stdio",
            "--command",
            "one",
            "--env",
            "TOKEN=first",
            "--env",
            "TOKEN=second",
        ],
    );

    let listed = mcp_ok(root.path(), &["list"]);
    assert!(
        listed.contains("No MCP servers configured."),
        "got: {listed}"
    );
}

/// What was typed for a malformed `--env` may be the value itself, so the
/// refusal does not repeat it.
#[test]
fn a_malformed_env_pair_is_refused_without_echoing_it() {
    let root = tempfile::tempdir().expect("temp data dir");

    let refused = mcp_err(
        root.path(),
        &[
            "add",
            "--name",
            "x",
            "--type",
            "stdio",
            "--command",
            "one",
            "--env",
            "sk-5f3a9c",
        ],
    );

    assert!(refused.contains("KEY=VALUE"), "got: {refused}");
    assert!(!refused.contains("sk-5f3a9c"), "echoed: {refused}");
}

/// A blank value is refused here as the page's add refuses it, by the one
/// rule the service holds, and nothing is stored.
#[test]
fn a_blank_env_value_is_refused_and_stores_nothing() {
    let root = tempfile::tempdir().expect("temp data dir");

    let refused = mcp_err(
        root.path(),
        &[
            "add",
            "--name",
            "x",
            "--type",
            "stdio",
            "--command",
            "one",
            "--env",
            "TOKEN=",
        ],
    );

    assert!(
        refused.contains("TOKEN is blank: enter a value or remove the variable"),
        "got: {refused}"
    );
    let listed = mcp_ok(root.path(), &["list"]);
    assert!(
        listed.contains("No MCP servers configured."),
        "got: {listed}"
    );
}

#[test]
fn an_unknown_type_is_refused_with_the_types_there_are() {
    let root = tempfile::tempdir().expect("temp data dir");

    let refused = mcp_err(
        root.path(),
        &["add", "--name", "odd", "--type", "grpc", "--url", "x"],
    );

    assert!(
        refused.contains("Invalid --type value: unknown server type 'grpc'; expected stdio or sse"),
        "got: {refused}"
    );
}

/// `manual` is no longer a lifecycle: the flag names the two there are, and
/// nothing is stored.
#[test]
fn a_manual_lifecycle_is_refused_with_the_lifecycles_there_are() {
    let root = tempfile::tempdir().expect("temp data dir");

    let refused = mcp_err(
        root.path(),
        &[
            "add",
            "--name",
            "files",
            "--type",
            "stdio",
            "--command",
            "true",
            "--lifecycle",
            "manual",
        ],
    );

    assert!(
        refused.contains("[possible values: eager, lazy]"),
        "got: {refused}"
    );
    let listed = mcp_ok(root.path(), &["list"]);
    assert!(
        listed.contains("No MCP servers configured."),
        "got: {listed}"
    );
}

/// The server is added with a bare command, so its row has no resolved path.
/// `mcp test` has to resolve it first, as the GUI's Test does: the client
/// refuses to start a path that is not absolute.
#[cfg(unix)]
#[test]
fn testing_a_newly_added_server_resolves_its_command_and_starts_it() {
    let root = tempfile::tempdir().expect("temp data dir");
    let script = root.path().join("stand-in.sh");
    std::fs::write(&script, STAND_IN_SERVER).expect("the stand-in is written");
    mcp_ok(
        root.path(),
        &[
            "add",
            "--name",
            "stand-in",
            "--type",
            "stdio",
            "--command",
            "sh",
            "--args",
            script.to_str().expect("a UTF-8 path"),
        ],
    );

    let tested = mcp_ok(root.path(), &["test", "stand-in"]);

    assert!(
        tested.contains("Connection successful — 1 tool(s) discovered"),
        "got: {tested}"
    );
    assert!(tested.contains("echo — says it back"), "got: {tested}");
}

/// What the GUI's Test reports for the same row: both are
/// `McpService::test_server`, whose failure this is.
#[test]
fn testing_a_server_whose_command_cannot_be_resolved_reports_the_failed_start() {
    let root = tempfile::tempdir().expect("temp data dir");
    mcp_ok(
        root.path(),
        &[
            "add",
            "--name",
            "ghost",
            "--type",
            "stdio",
            "--command",
            "gglib-no-such-command",
        ],
    );

    let failed = mcp_err(root.path(), &["test", "ghost"]);

    assert!(
        failed.contains("Failed to start MCP server")
            && failed.contains("Executable path must be absolute: gglib-no-such-command"),
        "got: {failed}"
    );
}

/// The cell of `listed`'s `Remote` column on the row for `name`.
fn remote_cell<'a>(listed: &'a str, name: &str) -> &'a str {
    cell(listed, name, "Remote")
}

/// The cell of `listed`'s column `heading` on the row for `name`.
fn cell<'a>(listed: &'a str, name: &str, heading: &str) -> &'a str {
    let mut lines = listed.lines();
    let column = lines
        .by_ref()
        .find_map(|line| line.split_whitespace().position(|cell| cell == heading))
        .unwrap_or_else(|| panic!("no {heading} column, got: {listed}"));
    lines
        .find(|line| line.split_whitespace().nth(1) == Some(name))
        .and_then(|row| row.split_whitespace().nth(column))
        .unwrap_or_else(|| panic!("no row for {name}, got: {listed}"))
}

/// A server is opened to paired devices and closed again by name or id, one
/// or several at a time, and the list says which are open.
#[test]
fn a_server_is_opened_to_paired_devices_and_closed_again() {
    let root = tempfile::tempdir().expect("temp data dir");
    let add = ["add", "--type", "stdio", "--command", "npx", "--name"];
    mcp_ok(root.path(), &[&add[..], &["tavily"]].concat());
    mcp_ok(root.path(), &[&add[..], &["fs"]].concat());
    let listed = mcp_ok(root.path(), &["list"]);
    assert_eq!(remote_cell(&listed, "tavily"), "no", "closed unless opened");

    let opened = mcp_ok(root.path(), &["allow-remote", "tavily", "fs"]);

    assert!(
        opened.contains("✓ 'tavily' is open to paired devices")
            && opened.contains("✓ 'fs' is open to paired devices"),
        "got: {opened}"
    );
    let listed = mcp_ok(root.path(), &["list"]);
    assert_eq!(remote_cell(&listed, "tavily"), "yes");
    assert_eq!(remote_cell(&listed, "fs"), "yes");

    let closed = mcp_ok(root.path(), &["deny-remote", "tavily"]);

    assert!(
        closed.contains("✓ 'tavily' is closed to paired devices"),
        "got: {closed}"
    );
    let listed = mcp_ok(root.path(), &["list"]);
    assert_eq!(remote_cell(&listed, "tavily"), "no");
    assert_eq!(remote_cell(&listed, "fs"), "yes", "only the one named");
}

/// A disabled server can be opened, and is said to offer nothing until it
/// is enabled.
#[test]
fn opening_a_disabled_server_says_it_offers_nothing_until_enabled() {
    let root = tempfile::tempdir().expect("temp data dir");
    let add = ["add", "--type", "stdio", "--command", "npx", "--disabled"];
    mcp_ok(root.path(), &[&add[..], &["--name", "off"]].concat());

    let opened = mcp_ok(root.path(), &["allow-remote", "off"]);

    assert!(
        opened.contains("✓ 'off' is open to paired devices")
            && opened.contains("It is disabled, so it offers them no tools"),
        "got: {opened}"
    );
}

/// An SSE server is never opened, and is said why, before any server named
/// beside it is opened; closing it is not refused.
#[test]
fn opening_an_sse_server_is_refused_with_the_reason() {
    let root = tempfile::tempdir().expect("temp data dir");
    stored_sse(root.path(), "remote", McpLifecycle::Lazy);
    data_dir::store_mcp_server(
        root.path(),
        NewMcpServer::new_stdio("fs", "npx", vec![], None),
    );

    let refused = mcp_err(root.path(), &["allow-remote", "fs", "remote"]);

    assert!(
        refused.contains(&format!("Error: {SSE_NOT_SUPPORTED}")),
        "got: {refused}"
    );
    let listed = mcp_ok(root.path(), &["list"]);
    assert_eq!(remote_cell(&listed, "remote"), "no");
    assert_eq!(remote_cell(&listed, "fs"), "no", "nothing changed");
    mcp_ok(root.path(), &["deny-remote", "remote"]);
}

/// A name two servers share opens neither, and points to their ids; an id
/// opens the one it names. Nothing is opened when any name is refused.
#[test]
fn a_name_two_servers_share_is_refused_and_an_id_is_asked_for() {
    let root = tempfile::tempdir().expect("temp data dir");
    // An older database can hold two of one name; `mcp add` refuses it now.
    for command in ["npx", "uvx"] {
        data_dir::store_mcp_server(
            root.path(),
            NewMcpServer::new_stdio("twin", command, vec![], None),
        );
    }
    data_dir::store_mcp_server(
        root.path(),
        NewMcpServer::new_stdio("fs", "npx", vec![], None),
    );

    let refused = mcp_err(root.path(), &["allow-remote", "fs", "twin"]);

    assert!(
        refused.contains("2 servers are named 'twin'; give its id (`gglib mcp list`)"),
        "got: {refused}"
    );
    let listed = mcp_ok(root.path(), &["list"]);
    assert_eq!(remote_cell(&listed, "fs"), "no", "nothing changed");

    let first_twin = listed
        .lines()
        .find(|line| line.split_whitespace().nth(1) == Some("twin"))
        .and_then(|row| row.split_whitespace().next())
        .expect("a twin's id");
    let opened = mcp_ok(root.path(), &["allow-remote", first_twin]);
    assert!(
        opened.contains("✓ 'twin' is open to paired devices"),
        "got: {opened}"
    );
}

/// `gglib remote enable --allow-mcp` is refused, pointing at where the grant
/// went, before any daemon is asked anything: the port it is pointed at
/// listens to nothing, and nothing is started on it.
#[test]
fn remote_enable_with_allow_mcp_is_refused_with_a_pointer() {
    let root = tempfile::tempdir().expect("temp data dir");
    let port = closed_port();
    let out = Command::new(env!("CARGO_BIN_EXE_gglib"))
        .args(["remote", "enable", "--allow-mcp", "t"])
        .env("GGLIB_DATA_DIR", root.path())
        .env("GGLIB_DAEMON_PORT", port.to_string())
        .stdin(Stdio::null())
        .output()
        .expect("running `gglib remote enable`");

    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success(), "stderr: {stderr}");
    assert!(
        stderr.contains("--allow-mcp is gone: run `gglib mcp allow-remote <names>`"),
        "stderr: {stderr}"
    );
    assert!(
        TcpListener::bind(("127.0.0.1", port)).is_ok(),
        "no daemon was started on the port"
    );
}

// ─── The daemon's copy ──────────────────────────────────────────────────────

/// A stand-in for the daemon: it says it is one on `/health`, answers each
/// request line in `routes` (`"GET /path"`) with that JSON, and anything
/// else with a 404. A route listed twice answers its first JSON once, then
/// its next. Every request line it read is in the log it hands back.
fn stand_in_daemon(mut routes: Vec<(String, String)>) -> (u16, Arc<Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
    let port = listener.local_addr().expect("its address").port();
    let log = Arc::new(Mutex::new(Vec::new()));
    let seen = Arc::clone(&log);
    std::thread::spawn(move || {
        while let Ok((request, mut stream)) = blocking::accept(&listener) {
            let line = request.line().to_owned();
            let reply = if line == "GET /health HTTP/1.1" {
                Some(r#"{"service":"gglib-daemon"}"#.to_owned())
            } else {
                let matching: Vec<usize> = (0..routes.len())
                    .filter(|&i| line == format!("{} HTTP/1.1", routes[i].0))
                    .collect();
                match matching.as_slice() {
                    [] => None,
                    [only] => Some(routes[*only].1.clone()),
                    [first, ..] => Some(routes.remove(*first).1),
                }
            };
            seen.lock().expect("the log").push(line);
            let (status, body) = reply.map_or(("404 Not Found", String::new()), |b| ("200 OK", b));
            let _ = write!(
                stream,
                "HTTP/1.1 {status}\r\ncontent-type: application/json\r\n\
                 content-length: {}\r\nconnection: close\r\n\r\n{body}",
                body.len()
            );
        }
    });
    (port, log)
}

/// The daemon's answer for server `id` named `name`, in `status`, offering
/// `tools` as `(name, description)`.
fn in_the_daemon(
    id: i64,
    name: &str,
    status: McpServerStatusDto,
    tools: &[(&str, &str)],
) -> McpServerInfo {
    McpServerInfo {
        server: McpServerDto {
            id,
            name: name.to_owned(),
            server_type: "stdio".to_owned(),
            config: McpServerConfigDto {
                command: Some("npx".to_owned()),
                resolved_path_cache: None,
                args: None,
                working_dir: None,
                path_extra: None,
                url: None,
            },
            enabled: true,
            allow_remote: false,
            lifecycle: McpLifecycle::Lazy,
            env_keys: Vec::new(),
            created_at: "2026-11-01T00:00:00+00:00".to_owned(),
            last_connected_at: None,
            is_valid: true,
            last_error: None,
        },
        status,
        tools: tools
            .iter()
            .map(|(name, description)| McpToolInfo {
                name: (*name).to_owned(),
                description: Some((*description).to_owned()),
                input_schema: None,
                title: None,
            })
            .collect(),
    }
}

/// The daemon's list route answering `infos`.
fn listing(infos: &[McpServerInfo]) -> (String, String) {
    (
        format!("GET {MCP_SERVERS_PATH}"),
        serde_json::to_string(infos).expect("the list as JSON"),
    )
}

/// The daemon's start route for `info`'s server, answering `info`.
fn starting(info: &McpServerInfo) -> (String, String) {
    (
        format!("POST {}", mcp_server_start_path(info.server.id)),
        serde_json::to_string(info).expect("the server as JSON"),
    )
}

/// Add a stdio server named `name` to `root` with `extra` flags, and answer
/// its id.
fn added(root: &Path, name: &str, extra: &[&str]) -> i64 {
    let add = ["add", "--type", "stdio", "--command", "npx", "--name", name];
    let said = mcp_ok(root, &[&add[..], extra].concat());
    said.split("(id: ")
        .nth(1)
        .and_then(|rest| rest.split(')').next())
        .and_then(|id| id.parse().ok())
        .unwrap_or_else(|| panic!("no id in: {said}"))
}

/// The request lines the stand-in read.
fn requests(log: &Arc<Mutex<Vec<String>>>) -> Vec<String> {
    log.lock().expect("the log").clone()
}

#[test]
fn list_shows_the_running_daemons_status_and_tools() {
    let root = tempfile::tempdir().expect("temp data dir");
    let id = added(root.path(), "fs", &[]);
    let running = in_the_daemon(
        id,
        "fs",
        McpServerStatusDto::Running,
        &[("read_file", "reads"), ("write_file", "writes")],
    );
    let (port, log) = stand_in_daemon(vec![listing(&[running])]);

    let listed = succeeded(&mcp_on(root.path(), port, &["list"]));

    assert_eq!(cell(&listed, "fs", "Status"), "running", "{listed}");
    assert_eq!(cell(&listed, "fs", "Tools"), "2", "{listed}");
    assert!(!listed.contains("unknown"), "{listed}");
    assert_eq!(
        requests(&log),
        [
            "GET /health HTTP/1.1".to_owned(),
            format!("GET {MCP_SERVERS_PATH} HTTP/1.1")
        ]
    );
}

/// The port is free afterwards: `list` asked nothing to start on it.
#[test]
fn list_with_no_daemon_shows_dashes_and_starts_none() {
    let root = tempfile::tempdir().expect("temp data dir");
    added(root.path(), "fs", &[]);
    let port = closed_port();

    let listed = succeeded(&mcp_on(root.path(), port, &["list"]));

    assert_eq!(cell(&listed, "fs", "Status"), "—", "{listed}");
    assert_eq!(cell(&listed, "fs", "Tools"), "—", "{listed}");
    assert!(
        listed.contains("Status and Tools are unknown: the daemon is not running."),
        "{listed}"
    );
    assert!(
        TcpListener::bind(("127.0.0.1", port)).is_ok(),
        "no daemon was started on the port"
    );
}

#[test]
fn tools_prints_what_the_daemons_running_copy_offers() {
    let root = tempfile::tempdir().expect("temp data dir");
    let id = added(root.path(), "fs", &[]);
    let running = in_the_daemon(
        id,
        "fs",
        McpServerStatusDto::Running,
        &[("read_file", "Read a file")],
    );
    let (port, log) = stand_in_daemon(vec![listing(&[running])]);

    let listed = succeeded(&mcp_on(root.path(), port, &["tools", "fs"]));

    assert!(listed.contains("1 tool(s) from 'fs'"), "{listed}");
    assert!(
        listed.contains("read_file") && listed.contains("Read a file"),
        "{listed}"
    );
    assert!(
        !requests(&log).iter().any(|line| line.starts_with("POST")),
        "a running server is not started again: {:?}",
        requests(&log)
    );
}

/// A stopped server is started in the daemon, as a chat's first call would
/// start it, and the tools printed are the ones that start answered.
#[test]
fn tools_starts_a_stopped_server_in_the_daemon() {
    let root = tempfile::tempdir().expect("temp data dir");
    let id = added(root.path(), "fs", &[]);
    let stopped = in_the_daemon(id, "fs", McpServerStatusDto::Stopped, &[]);
    let started = in_the_daemon(
        id,
        "fs",
        McpServerStatusDto::Running,
        &[("list_directory", "Lists a folder")],
    );
    let (port, log) = stand_in_daemon(vec![listing(&[stopped]), starting(&started)]);

    let listed = succeeded(&mcp_on(root.path(), port, &["tools", "fs"]));

    assert!(listed.contains("1 tool(s) from 'fs'"), "{listed}");
    assert!(listed.contains("list_directory"), "{listed}");
    assert_eq!(
        requests(&log),
        [
            "GET /health HTTP/1.1".to_owned(),
            format!("GET {MCP_SERVERS_PATH} HTTP/1.1"),
            format!("POST {} HTTP/1.1", mcp_server_start_path(id)),
        ]
    );
}

/// A chat's lazy start can land between `tools`' read and its own start,
/// which then fails as "already running". The server is up, so its tools are
/// read again and printed rather than the start's error.
#[test]
fn tools_reads_again_when_a_chat_started_the_server_first() {
    let root = tempfile::tempdir().expect("temp data dir");
    let id = added(root.path(), "fs", &[]);
    let stopped = in_the_daemon(id, "fs", McpServerStatusDto::Stopped, &[]);
    let running = in_the_daemon(
        id,
        "fs",
        McpServerStatusDto::Running,
        &[("list_directory", "Lists a folder")],
    );
    // No start route: the start answers 404, as a refused start would fail.
    let (port, log) = stand_in_daemon(vec![listing(&[stopped]), listing(&[running])]);

    let listed = succeeded(&mcp_on(root.path(), port, &["tools", "fs"]));

    assert!(listed.contains("list_directory"), "{listed}");
    assert_eq!(
        requests(&log),
        [
            "GET /health HTTP/1.1".to_owned(),
            format!("GET {MCP_SERVERS_PATH} HTTP/1.1"),
            format!("POST {} HTTP/1.1", mcp_server_start_path(id)),
            format!("GET {MCP_SERVERS_PATH} HTTP/1.1"),
        ]
    );
}

/// The stand-in would start it if asked; it is asked nothing.
#[test]
fn tools_of_a_disabled_server_says_so_and_starts_nothing() {
    let root = tempfile::tempdir().expect("temp data dir");
    let id = added(root.path(), "off", &["--disabled"]);
    let stopped = in_the_daemon(id, "off", McpServerStatusDto::Stopped, &[]);
    let started = in_the_daemon(id, "off", McpServerStatusDto::Running, &[("t", "d")]);
    let (port, log) = stand_in_daemon(vec![listing(&[stopped]), starting(&started)]);

    let said = succeeded(&mcp_on(root.path(), port, &["tools", "off"]));

    assert!(
        said.contains("'off' is disabled, so chats don't get its tools"),
        "{said}"
    );
    assert_eq!(requests(&log), Vec::<String>::new());
}

/// No alias: a lazy server starts itself, `eager` starts one at init, and
/// `disable` keeps one out of chats (#1689).
#[test]
fn start_and_stop_are_not_commands() {
    let root = tempfile::tempdir().expect("temp data dir");

    for command in ["start", "stop"] {
        let refused = mcp_err(root.path(), &[command, "fs"]);

        assert!(
            refused.contains(&format!("unrecognized subcommand '{command}'")),
            "got: {refused}"
        );
    }
}
