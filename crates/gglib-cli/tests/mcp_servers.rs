//! `gglib mcp add`, `list`, `start`, `test`, `enable`, `disable` and
//! `remove`, run as a person runs them against a database on disk, and what
//! `gglib chat` and `gglib q` say of a stored server they cannot run.
//!
//! The unit tests hand the service a repository they built; these run the
//! binary, so the tables are the ones its own bootstrap creates under
//! `GGLIB_DATA_DIR`. Nothing here reaches a network: the one server that is
//! started is a shell script in the test's own directory, and the chat
//! commands are pointed at a port nothing listens on.

use std::net::TcpListener;
use std::path::Path;
use std::process::{Command, Output, Stdio};

use gglib_core::domain::mcp::{McpLifecycle, NewMcpServer};

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
/// stdin.
fn printed(root: &Path, args: &[&str]) -> (String, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_gglib"))
        .args(args)
        .env("GGLIB_DATA_DIR", root)
        .env_remove("RUST_LOG")
        .stdin(Stdio::null())
        .output()
        .unwrap_or_else(|e| panic!("running `gglib {}`: {e}", args.join(" ")));
    let text = |bytes: &[u8]| String::from_utf8_lossy(bytes).into_owned();
    (text(&out.stdout), text(&out.stderr))
}

/// `gglib mcp <args>` over the data directory `root`.
fn mcp(root: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_gglib"))
        .arg("mcp")
        .args(args)
        .env("GGLIB_DATA_DIR", root)
        .output()
        .unwrap_or_else(|e| panic!("running `gglib mcp {}`: {e}", args.join(" ")))
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

    for command in ["start", "test", "enable", "disable"] {
        let refused = mcp_err(root.path(), &[command, "remote"]);

        assert!(
            refused.contains(&format!("Error: {SSE_NOT_SUPPORTED}")),
            "`mcp {command}` got: {refused}"
        );
    }

    let removed = mcp_ok(root.path(), &["remove", "remote", "--force"]);
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
