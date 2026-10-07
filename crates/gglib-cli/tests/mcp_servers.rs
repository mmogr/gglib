//! `gglib mcp add`, `list` and `test`, run as a person runs them against a
//! database on disk.
//!
//! The unit tests hand the service a repository they built; these run the
//! binary, so the tables are the ones its own bootstrap creates under
//! `GGLIB_DATA_DIR`. Nothing here reaches a network: the one server that is
//! started is a shell script in the test's own directory.

use std::path::Path;
use std::process::{Command, Output};

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

#[test]
fn an_sse_server_is_added_and_listed() {
    let root = tempfile::tempdir().expect("temp data dir");

    let added = mcp_ok(
        root.path(),
        &[
            "add",
            "--name",
            "remote",
            "--type",
            "sse",
            "--url",
            "http://localhost:3001/sse",
        ],
    );
    assert!(
        added.contains("Added MCP server 'remote' (id: 1)"),
        "got: {added}"
    );

    let listed = mcp_ok(root.path(), &["list"]);
    assert!(listed.contains("Found 1 MCP server(s)"), "got: {listed}");
    let row = listed
        .lines()
        .find(|line| line.contains("remote"))
        .unwrap_or_else(|| panic!("no row for the server, got: {listed}"));
    assert!(row.contains(" sse "), "the row names its type, got: {row}");
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
