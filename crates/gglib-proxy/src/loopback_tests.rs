//! Every HTTP client the crates that talk to this machine build is built by
//! `loopback`, read off their sources by `loopback_scanner`: a
//! `reqwest::Client::new()`, `reqwest::Client::builder()` or
//! `reqwest::ClientBuilder::new()` anywhere else in them would honour a proxy
//! on its way to `127.0.0.1`. The reader's own pieces each have a test here.

use std::fs;
use std::path::PathBuf;

use super::loopback_scanner::{
    CONTROL, HOME, INTERNET, TREES, builds_a_client, imports, offenders_in, production_lines,
    rust_sources, workspace_root,
};

#[test]
fn every_client_for_this_machine_is_built_here() {
    let root = workspace_root();
    let mut files = Vec::new();
    for tree in TREES {
        rust_sources(&root.join(tree), &mut files);
    }
    let control = root.join(CONTROL);
    assert!(files.contains(&control), "the scan did not reach {CONTROL}");
    let control_text = fs::read_to_string(&control).expect("read the control file");
    assert!(
        control_text.contains("loopback::client_builder()"),
        "{CONTROL} no longer builds its upstream client through loopback, so this scan's \
         positive control is gone"
    );

    let home: Vec<PathBuf> = HOME.iter().map(|path| root.join(path)).collect();
    let internet: Vec<PathBuf> = INTERNET.iter().map(|(path, _)| root.join(path)).collect();
    for path in &home {
        assert!(
            files.contains(path),
            "a file named in HOME is gone: {}",
            path.display()
        );
    }
    for path in &internet {
        assert!(
            files.contains(path),
            "an internet client named in INTERNET is gone: {}",
            path.display()
        );
    }
    let offenders: Vec<String> = files
        .iter()
        .filter(|file| !home.contains(file) && !internet.contains(file))
        .flat_map(|file| offenders_in(&root, file))
        .collect();
    assert!(
        offenders.is_empty(),
        "a client for this machine built outside gglib_proxy::loopback, which would honour a \
         proxy on its way to 127.0.0.1:\n{}",
        offenders.join("\n")
    );
}

#[test]
fn the_predicate_sees_every_spelling_and_no_other_client() {
    for line in [
        "    let c = reqwest::Client::new();",
        "    reqwest::Client::builder().timeout(t)",
        "    let b = reqwest::ClientBuilder::new();",
    ] {
        assert!(builds_a_client(line, false, false), "{line}");
    }
    assert!(builds_a_client("    let c = Client::new();", true, false));
    assert!(builds_a_client(
        "    Self::with_client(url, Client::new(), model)",
        true,
        false
    ));
    assert!(builds_a_client(
        "    let b = ClientBuilder::new();",
        false,
        true
    ));
    assert!(!builds_a_client("    let c = Client::new();", false, false));
    assert!(!builds_a_client(
        "    let d = DaemonClient::new();",
        true,
        true
    ));
    assert!(!builds_a_client(
        "    let m = McpClient::builder();",
        true,
        true
    ));
}

#[test]
fn the_import_check_reads_both_forms() {
    assert!(imports("use reqwest::Client;\n", "Client"));
    assert!(imports("use reqwest::{Client, StatusCode};\n", "Client"));
    assert!(imports("use reqwest::ClientBuilder;\n", "ClientBuilder"));
    assert!(!imports("use reqwest::StatusCode;\n", "Client"));
    assert!(!imports("use hf_hub::Client;\n", "Client"));
}

#[test]
fn production_lines_skip_every_test_item_and_keep_the_code_between() {
    let text = "\
fn a() {}
#[cfg(test)]
mod tests {
    /// Verbatim JSON, braces unbalanced on purpose: {\"slots\": [{
    const SAMPLE: &str = \"{ { {\";
    fn t() { let x = 1; }
} // the fixtures end here
fn b() {}
#[cfg(test)]
#[path = \"x_tests.rs\"]
mod x_tests;
fn c() {}
impl Thing {
    #[cfg(test)]
    fn only_in_tests(&self) {
        let _ = 1;
    }
    fn d() {}
}
#[cfg(all(test, unix))]
mod unix_tests {
    fn v() {}
}
#[cfg(not(test))]
fn kept_because_not_test() {}
#[cfg(test)]
mod more {
    mod nested { fn u() {} }
}
fn e() {}
";
    let kept: Vec<&str> = production_lines(text).into_iter().map(|(_, l)| l).collect();
    assert_eq!(
        kept,
        [
            "fn a() {}",
            "fn b() {}",
            "fn c() {}",
            "impl Thing {",
            "    fn d() {}",
            "}",
            "#[cfg(not(test))]",
            "fn kept_because_not_test() {}",
            "fn e() {}"
        ]
    );
}

#[test]
fn this_machine_is_localhost_or_a_loopback_address() {
    for host in [
        "127.0.0.1",
        "localhost",
        "LOCALHOST",
        "localhost.",
        "::1",
        "[::1]",
        "0:0:0:0:0:0:0:1",
        "::ffff:127.0.0.1",
        "[::ffff:127.0.0.1]",
        "127.0.0.2",
    ] {
        assert!(super::is_this_machine(host), "{host}");
    }
    // `127.1` is a resolver shorthand this deliberately does not read.
    for host in [
        "192.168.1.20",
        "gglib.local",
        "example.com",
        "0.0.0.0",
        "127.1",
        "",
    ] {
        assert!(!super::is_this_machine(host), "{host}");
    }
}
