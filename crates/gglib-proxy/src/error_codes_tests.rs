//! `docs/error-codes.json` against the table, and the table against the codes
//! the crates write (#1119).
//!
//! Read: every `crates/*/src` tree, less test code: `*_tests.rs`, `tests.rs`,
//! `*_fixture(s).rs` and `test_*.rs` files (each a module declared under
//! `#[cfg(test)]`), the `gglib-integration-tests` crate, and this module's
//! own files, which spell out every code. Not here: the daemon's guards and
//! its other `/api` routes, whose refusals name a `type` such as
//! `HOST_NOT_ALLOWED` and no `code`, and the MCP gateway's JSON-RPC errors,
//! whose codes are numbers.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use super::error_codes_scan::{Write, scan};
use super::{CODES, Kind, PUBLISHED, render};

/// Every relay the scan finds, one row a site: the file under `crates/`, the
/// `fn`, the code's spelling there, and where the code comes from. The last
/// three hold a `code` of another kind, which the scan cannot tell apart.
#[rustfmt::skip]
const RELAYS: [(&str, &str, &str, &str); 20] = [
    ("gglib-proxy/src/models.rs", "with_code", "Some(code.into())", "`with_code`'s parameter"),
    ("gglib-proxy/src/runs/handlers.rs", "error", "code", "`error`'s parameter"),
    ("gglib-proxy/src/runs/handlers.rs", "refused", "err.code()", "`RunsError::code`"),
    ("gglib-proxy/src/chats/handlers.rs", "refused", "err.code()", "`HubChatsError::code`"),
    ("gglib-proxy/src/runs/turn.rs", "refused", "&refusal.code", "the daemon's `TurnRefused`"),
    ("gglib-proxy/src/sse_stream.rs", "spawn_and_return", "code", "llama-server's, else `upstream_error`"),
    ("gglib-core/src/sse/encoder.rs", "upstream_error_frame", "code", "`upstream_error_frame`'s parameter"),
    ("gglib-core/src/sse/encoder.rs", "encode", "code", "an `UpstreamError` event's"),
    ("gglib-core/src/sse/parser.rs", "parse_inline_error_frame", "code", "llama-server's, else `upstream_error`"),
    ("gglib-app-services/src/runs/chat.rs", "refusal", "code", "the proxy's refusal, else `upstream_error`"),
    ("gglib-app-services/src/runs/chat.rs", "in_stream_error", "code", "the proxy's error frame, else `upstream_error`"),
    ("gglib-app-services/src/runs/chat.rs", "run_error", "code.to_owned()", "`run_error`'s parameter"),
    ("gglib-axum/src/handlers/agent/run.rs", "coded", "code", "`coded`'s parameter"),
    ("gglib-axum/src/handlers/agent/run.rs", "run_error", "code.to_owned()", "the `AgentError` arms above it"),
    ("gglib-axum/src/handlers/agent/transcript.rs", "coded", "code", "`coded`'s parameter"),
    ("gglib-axum/src/handlers/agent/hub_turn.rs", "refusal", "code.to_owned()", "a `Coded` refusal's"),
    ("gglib-axum/src/error.rs", "from", "e.code()", "`RunsError::code`"),
    ("gglib-cli/src/handlers/remote/pairing_tui.rs", "qr", "QrCode::new(pairing.to_uppercase()).ok()?", "a QR code, not an error's"),
    ("gglib-cli/src/handlers/remote/pairing_tui.rs", "draw", "enabled.code.as_deref().unwrap_or_default()", "a pairing code, not an error's"),
    ("gglib-runtime/src/ports_impl/llm_completion/retry/classify.rs", "classify", "err.error.code", "an upstream body's, kept to retry by, not written"),
];

/// A write, its file under `crates/`, and its line.
struct Found {
    write: Write,
    file: String,
    line: usize,
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("the workspace root above crates/gglib-proxy")
}

fn is_test_file(name: &str) -> bool {
    let fixture = name.ends_with("_fixtures.rs") || name.ends_with("_fixture.rs");
    name.ends_with("_tests.rs") || name == "tests.rs" || fixture || name.starts_with("test_")
}

fn sources(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).expect("read a source directory") {
        let path = entry.expect("a directory entry").path();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if path.is_dir() {
            sources(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs")
            && !is_test_file(name)
            && !name.starts_with("error_codes")
        {
            out.push(path);
        }
    }
}

/// Every write in every file read.
fn scan_tree() -> Vec<Found> {
    let crates = workspace_root().join("crates");
    let mut files = Vec::new();
    for entry in fs::read_dir(&crates).expect("read crates/") {
        let krate = entry.expect("a crate").path();
        if krate.join("src").is_dir() && !krate.ends_with("gglib-integration-tests") {
            sources(&krate.join("src"), &mut files);
        }
    }
    files.sort();
    let mut found = Vec::new();
    for path in files {
        let text = fs::read_to_string(&path).expect("read a source file");
        let file = path.strip_prefix(&crates).unwrap_or(&path).display();
        let file = file.to_string().replace('\\', "/");
        found.extend(scan(&text).into_iter().map(|(write, line)| Found {
            write,
            file: file.clone(),
            line,
        }));
    }
    found
}

static TREE: LazyLock<Vec<Found>> = LazyLock::new(scan_tree);

fn written() -> impl Iterator<Item = (&'static Found, &'static str, Option<&'static str>)> {
    TREE.iter().filter_map(|f| match &f.write {
        Write::Code(code, kind) => Some((f, code.as_str(), kind.as_deref())),
        Write::Relay(..) => None,
    })
}

#[test]
fn the_published_file_is_the_table() {
    let path = workspace_root().join(PUBLISHED);
    let expected = render();
    if std::env::var_os("GGLIB_WRITE_ERROR_CODES").is_some() {
        fs::write(&path, &expected).expect("write the published file");
    }
    let published = fs::read_to_string(&path)
        .unwrap_or_default()
        .replace("\r\n", "\n");
    assert!(
        published == expected,
        "{PUBLISHED} disagrees with the table in crates/gglib-proxy/src/error_codes.rs; \
         rewrite it with GGLIB_WRITE_ERROR_CODES=1 cargo test -p gglib-proxy error_codes"
    );
}

#[test]
fn every_code_a_crate_writes_is_in_the_table() {
    let listed: BTreeSet<&str> = CODES.iter().map(|c| c.code).collect();
    let unlisted: Vec<String> = written()
        .filter(|(_, code, _)| !listed.contains(code))
        .map(|(f, code, _)| format!("{}:{}: {code}", f.file, f.line))
        .collect();
    assert!(
        unlisted.is_empty(),
        "written but not in error_codes.rs's table: {unlisted:#?}"
    );
}

#[test]
fn every_code_in_the_table_is_written() {
    let codes: BTreeSet<&str> = written().map(|(_, code, _)| code).collect();
    let unwritten: Vec<&str> = CODES
        .iter()
        .map(|c| c.code)
        .filter(|c| !codes.contains(c))
        .collect();
    assert!(
        unwritten.is_empty(),
        "in the table but written nowhere: {unwritten:?}"
    );
}

#[test]
fn a_type_written_beside_a_code_is_the_tables() {
    let kinds: BTreeMap<&str, Kind> = CODES.iter().map(|c| (c.code, c.kind)).collect();
    let wrong: Vec<String> = written()
        .filter(|(_, code, kind)| match (kinds.get(code), kind) {
            (Some(Kind::One(listed)), Some(kind)) => listed != kind,
            (Some(Kind::Untyped), Some(_)) => true,
            _ => false,
        })
        .map(|(f, code, kind)| format!("{}:{}: {code} as {kind:?}", f.file, f.line))
        .collect();
    assert!(
        wrong.is_empty(),
        "a type the table does not give: {wrong:#?}"
    );
}

#[test]
fn every_relay_is_one_named_here() {
    let mut count: BTreeMap<(&str, &str, &str), i32> = BTreeMap::new();
    for f in TREE.iter() {
        if let Write::Relay(func, spelling) = &f.write {
            let site = (f.file.as_str(), func.as_str(), spelling.as_str());
            *count.entry(site).or_default() += 1;
        }
    }
    for &(file, func, spelling, _) in &RELAYS {
        *count.entry((file, func, spelling)).or_default() -= 1;
    }
    let off: Vec<_> = count.into_iter().filter(|&(_, n)| n != 0).collect();
    assert!(
        off.is_empty(),
        "relay sites less RELAYS rows, by file, fn and spelling (above 0, a site no row \
         names; below, a row no site matches): {off:#?}"
    );
}

#[test]
fn the_table_names_each_code_once() {
    let mut seen = BTreeSet::new();
    let twice: Vec<&str> = CODES
        .iter()
        .map(|c| c.code)
        .filter(|c| !seen.insert(*c))
        .collect();
    assert!(twice.is_empty(), "named twice: {twice:?}");
}
