//! The reader behind `loopback_tests`: which source trees are scanned, which
//! files are exempt and why, what counts as building a reqwest client, and
//! which lines of a file are production code. It names the spellings it looks
//! for, so it is exempt from the scan itself, beside `loopback.rs`.
//!
//! Test code is left out of a scan: `*_tests.rs` siblings, `tests/`
//! directories (the trees are `src` roots), and every item under a
//! `#[cfg(test)]` or `#[cfg(all(test, …))]` attribute, which a file may carry
//! more than once. A test may build a client with reqwest's defaults on
//! purpose, as `health_proxy_tests` does for its positive control. The files
//! whose clients fetch from the internet are named here, each with its
//! reason; a client there must honour a proxy.

use std::fs;
use std::path::{Path, PathBuf};

/// The source trees whose clients all talk to this machine. `gglib-download`
/// and `gglib-hf` are not here: their clients fetch from the internet and must
/// honour a proxy.
pub(super) const TREES: [&str; 6] = [
    "crates/gglib-proxy/src",
    "crates/gglib-runtime/src",
    "crates/gglib-cli/src",
    "crates/gglib-app-services/src",
    "crates/gglib-axum/src",
    "src-tauri/src",
];

/// The one file allowed to build a client for this machine the ordinary way,
/// and this reader, which spells out what it looks for.
pub(super) const HOME: [&str; 2] = [
    "crates/gglib-proxy/src/loopback.rs",
    "crates/gglib-proxy/src/loopback_scanner.rs",
];

/// Files whose clients talk to the internet, not to this machine, and so are
/// built the ordinary way on purpose. Each names what it fetches.
pub(super) const INTERNET: [(&str, &str); 1] = [(
    "crates/gglib-runtime/src/llama/download/mod.rs",
    "llama-server release assets from api.github.com and its download host",
)];

/// A file that must be scanned and must use the builder, so that a scan that
/// found nothing cannot pass by reading nothing.
pub(super) const CONTROL: &str = "crates/gglib-proxy/src/server.rs";

pub(super) fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("the workspace root above crates/gglib-proxy")
}

pub(super) fn rust_sources(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).expect("read a source directory") {
        let path = entry.expect("a directory entry").path();
        if path.is_dir() {
            rust_sources(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs")
            && !path
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.ends_with("_tests.rs"))
        {
            out.push(path);
        }
    }
}

/// Whether `text` imports reqwest's `Client` or `ClientBuilder` by name, so
/// that a bare `Client::new()` in it is reqwest's and not some other client's.
pub(super) fn imports(text: &str, name: &str) -> bool {
    text.lines().any(|line| {
        let line = line.trim();
        line.starts_with("use reqwest::")
            && (line.ends_with(&format!("::{name};"))
                || (line.contains('{') && line.contains(name)))
    })
}

/// Whether `line` builds a reqwest client the ordinary way. The qualified
/// spellings always count; the bare ones only when the file imports the type
/// from reqwest, since `DaemonClient::new()` is not a client of this kind.
pub(super) fn builds_a_client(line: &str, bare_client: bool, bare_builder: bool) -> bool {
    line.contains("reqwest::Client::new()")
        || line.contains("reqwest::Client::builder()")
        || line.contains("reqwest::ClientBuilder::new()")
        || (bare_client
            && (line.contains(" Client::new()")
                || line.contains("(Client::new()")
                || line.contains(" Client::builder()")))
        || (bare_builder && line.contains(" ClientBuilder::new()"))
}

/// The production lines of a source file: everything except the items under a
/// `#[cfg(test)]` or `#[cfg(all(test, …))]` attribute. Attributes between the
/// gate and the item belong to it. A braced item runs to the line that is its
/// closing brace at the item's own indentation, a trailing `//` comment
/// allowed, which is how rustfmt (enforced in CI) lays out every item; braces
/// are not counted, because a test module full of JSON in strings and doc
/// comments has no reason to balance them line by line. An unbraced item
/// (`mod tests;`, a `use`) is the one line.
///
/// Read line by line, not parsed: a line inside a string literal that spells
/// a gate or a closing brace would be taken for one. None does today, and a
/// gate hidden in a fixture would only ever hide code, so a source fixture
/// added to a scanned crate is the moment to reconsider this.
pub(super) fn is_test_gate(line: &str) -> bool {
    let line = line.trim();
    line == "#[cfg(test)]" || line.starts_with("#[cfg(all(test,")
}

pub(super) fn closes(line: &str, closing: &str) -> bool {
    let Some(rest) = line.strip_prefix(closing) else {
        return false;
    };
    let rest = rest.trim_start();
    rest.is_empty() || rest.starts_with("//")
}

pub(super) fn production_lines(text: &str) -> Vec<(usize, &str)> {
    let mut kept = Vec::new();
    let mut lines = text.lines().enumerate().peekable();
    while let Some((index, line)) = lines.next() {
        if !is_test_gate(line) {
            kept.push((index, line));
            continue;
        }
        // Skip the item: attribute lines, then the item itself.
        let Some((_, item)) = lines.find(|(_, l)| !l.trim().starts_with("#[")) else {
            break;
        };
        let trimmed = item.trim_end();
        if !trimmed.ends_with('{') {
            continue; // a one-line item, or `mod tests { }` closed on its line
        }
        let indent = &item[..item.len() - item.trim_start().len()];
        let closing = format!("{indent}}}");
        for (_, l) in lines.by_ref() {
            if closes(l, &closing) {
                break;
            }
        }
    }
    kept
}

pub(super) fn offenders_in(root: &Path, file: &Path) -> Vec<String> {
    let text = fs::read_to_string(file).expect("read a source file");
    let bare_client = imports(&text, "Client");
    let bare_builder = imports(&text, "ClientBuilder");
    production_lines(&text)
        .into_iter()
        .filter(|(_, line)| builds_a_client(line, bare_client, bare_builder))
        .map(|(index, line)| {
            let shown = file.strip_prefix(root).unwrap_or(file).display();
            format!("{shown}:{}: {}", index + 1, line.trim())
        })
        .collect()
}
