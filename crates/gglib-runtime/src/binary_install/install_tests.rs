//! The pipeline end to end against a loopback GitHub: what it asks for, what
//! it streams, what it leaves on disk and what it records.

use std::cell::RefCell;
use std::io::{Cursor, Write};
use std::path::PathBuf;

use anyhow::Result;
use tokio::sync::mpsc;

use super::fake_github::{FakeGitHub, Reply};
use super::install::install_prebuilt_from;
use super::*;
use crate::llama::LlamaProgressEvent;

thread_local! {
    /// The download directory of the test running on this thread. Each
    /// `#[tokio::test]` runs on a current-thread runtime of its own, and the
    /// pipeline never leaves it.
    static DOWNLOADS: RefCell<PathBuf> = const { RefCell::new(PathBuf::new()) };
}

#[allow(
    clippy::unnecessary_wraps,
    reason = "a ReleaseSpec's download_dir is fallible"
)]
fn test_downloads() -> Result<PathBuf> {
    Ok(DOWNLOADS.with(|d| d.borrow().clone()))
}

fn everything(_: &str) -> bool {
    true
}

const LISTING: &str = "/repos/acme/widget.cpp/releases/tags/v1";

fn spec(choice: AssetChoice) -> ReleaseSpec {
    ReleaseSpec {
        product: "widget.cpp",
        repo: "acme/widget.cpp",
        pinned: "v1",
        env: "GGLIB_WIDGET_RELEASE",
        download_dir: test_downloads,
        archive: ArchiveLayout::Flat,
        choice,
        wanted: everything,
    }
}

fn pinned() -> ReleaseSelector {
    ReleaseSelector::Tag("v1".to_owned())
}

/// A zip holding `names`, each a few bytes.
fn zip_of(names: &[&str]) -> Vec<u8> {
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let options =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    for name in names {
        zip.start_file(*name, options).expect("a member");
        zip.write_all(b"fixture").expect("its bytes");
    }
    zip.finish().expect("finish the zip").into_inner()
}

/// Serve release `v1` listing `assets`, each downloadable as `zip`.
async fn serve(assets: &[&str], zip: &[u8]) -> FakeGitHub {
    let fake = FakeGitHub::serve().await;
    let listed: Vec<serde_json::Value> = assets
        .iter()
        .map(|name| {
            fake.route(&format!("/download/{name}"), Reply::ok(zip.to_vec()));
            serde_json::json!({
                "name": name,
                "browser_download_url": format!("{}/download/{name}", fake.base),
            })
        })
        .collect();
    let body = serde_json::json!({ "tag_name": "v1", "assets": listed }).to_string();
    fake.route(LISTING, Reply::ok(body));
    fake
}

/// A temp root, and the directories in it this thread's run uses.
struct Dirs {
    _root: tempfile::TempDir,
    bin: PathBuf,
    server: PathBuf,
    downloads: PathBuf,
}

fn dirs() -> Dirs {
    let root = tempfile::tempdir().expect("a temp dir");
    let bin = root.path().join("bin");
    let downloads = root.path().join("downloads");
    DOWNLOADS.with(|d| *d.borrow_mut() = downloads.clone());
    Dirs {
        server: bin.join("widget-server"),
        _root: root,
        bin,
        downloads,
    }
}

fn target<'a>(dirs: &'a Dirs, matcher: AssetMatcher<'a>) -> PrebuiltTarget<'a> {
    PrebuiltTarget {
        matcher,
        description: "Testland (Widget)",
        required: &["widget-server", "libwidget.so"],
        bin_dir: &dirs.bin,
        server_path: &dirs.server,
        cuda_runtime: None,
    }
}

const LINUX: AssetMatcher<'static> = AssetMatcher {
    contains: &["-bin-Linux-"],
    ends_with: Some("-x86_64.zip"),
};

/// What one run did: its result, the events it sent, and the record it
/// handed over.
struct Run {
    result: Result<()>,
    events: Vec<LlamaProgressEvent>,
    record: Option<PrebuiltRecord>,
}

async fn run(fake: &FakeGitHub, spec: &ReleaseSpec, target: PrebuiltTarget<'_>) -> Run {
    let (tx, mut rx) = mpsc::channel(256);
    let mut record = None;
    let result = install_prebuilt_from(
        &fake.base,
        &pinned(),
        spec,
        target,
        |r| {
            record = Some(r);
            Ok(())
        },
        &tx,
    )
    .await;
    drop(tx);
    let mut events = Vec::new();
    while let Some(event) = rx.recv().await {
        events.push(event);
    }
    Run {
        result,
        events,
        record,
    }
}

/// The events as frames, with every progress frame but the last left out:
/// how many the throttle lets through depends on timing.
fn frames(events: &[LlamaProgressEvent]) -> Vec<String> {
    let last_progress = events
        .iter()
        .rposition(|e| matches!(e, LlamaProgressEvent::Progress { .. }));
    events
        .iter()
        .enumerate()
        .filter(|(i, e)| {
            !matches!(e, LlamaProgressEvent::Progress { .. }) || Some(*i) == last_progress
        })
        .map(|(_, e)| match e {
            LlamaProgressEvent::Progress {
                downloaded, total, ..
            } => format!("progress {downloaded}/{total}"),
            other => serde_json::to_string(other).expect("a frame"),
        })
        .collect()
}

fn phase(kind: &str, phase: &str) -> String {
    format!(r#"{{"type":"phase_{kind}","phase":"{phase}"}}"#)
}

#[tokio::test]
async fn a_release_installs_end_to_end_and_leaves_no_archive() {
    let dirs = dirs();
    let names = [
        "w-v1-bin-Darwin-macOS-26.6.2-arm64.zip",
        "w-v1-bin-Linux-Ubuntu-24.04-x86_64-vulkan.zip",
        "w-v1-bin-Linux-Ubuntu-24.04-x86_64.zip",
    ];
    let zip = zip_of(&["widget-server", "libwidget.so", "widget-cli"]);
    let fake = serve(&names, &zip).await;

    let run = run(&fake, &spec(AssetChoice::ExactlyOne), target(&dirs, LINUX)).await;

    run.result.expect("the install succeeds");
    let size = zip.len();
    assert_eq!(
        frames(&run.events),
        [
            phase("started", "fetch_release"),
            phase("completed", "fetch_release"),
            phase("started", "download"),
            format!("progress {size}/{size}"),
            phase("completed", "download"),
            phase("started", "extract"),
            phase("completed", "extract"),
            phase("started", "verify"),
            phase("completed", "verify"),
            r#"{"type":"completed","version":"v1"}"#.to_owned(),
        ]
    );
    assert_eq!(
        fake.asked(),
        [
            LISTING.to_owned(),
            "/download/w-v1-bin-Linux-Ubuntu-24.04-x86_64.zip".to_owned(),
        ]
    );
    let record = run.record.expect("a record was handed over");
    assert_eq!(
        (
            record.version.as_str(),
            record.platform.as_str(),
            record.install_type.as_str()
        ),
        ("v1", "Testland (Widget)", "prebuilt")
    );
    for name in ["widget-server", "libwidget.so", "widget-cli"] {
        assert!(dirs.bin.join(name).is_file(), "{name} was not unpacked");
    }
    assert!(!dirs.downloads.exists(), "the downloads directory is left");
}

#[tokio::test]
async fn a_missing_tag_names_the_product_and_its_override() {
    let dirs = dirs();
    let fake = FakeGitHub::serve().await;

    let run = run(&fake, &spec(AssetChoice::First), target(&dirs, LINUX)).await;

    assert_eq!(
        run.result.expect_err("no such release").to_string(),
        "widget.cpp release 'v1' not found upstream. \
         Set GGLIB_WIDGET_RELEASE=latest to install the current release, \
         or GGLIB_WIDGET_RELEASE=<tag> to name a different one."
    );
    assert_eq!(fake.asked(), [LISTING]);
    assert_eq!(frames(&run.events), [phase("started", "fetch_release")]);
}

#[tokio::test]
async fn two_matching_assets_are_refused_when_one_is_required_and_nothing_is_downloaded() {
    let dirs = dirs();
    let names = [
        "w-v1-bin-Linux-Ubuntu-24.04-x86_64.zip",
        "w-v1-bin-Linux-Ubuntu-22.04-x86_64.zip",
    ];
    let fake = serve(&names, &zip_of(&["widget-server"])).await;

    let run = run(&fake, &spec(AssetChoice::ExactlyOne), target(&dirs, LINUX)).await;

    assert_eq!(
        run.result.expect_err("ambiguous").to_string(),
        "More than one asset matches pattern '-bin-Linux-*-x86_64.zip' in release v1: \
         w-v1-bin-Linux-Ubuntu-24.04-x86_64.zip, w-v1-bin-Linux-Ubuntu-22.04-x86_64.zip"
    );
    assert_eq!(fake.asked(), [LISTING]);
    assert!(run.record.is_none());
}

#[tokio::test]
async fn the_first_matching_asset_is_taken_when_the_first_will_do() {
    let dirs = dirs();
    let names = [
        "w-v1-bin-Linux-Ubuntu-24.04-x86_64.zip",
        "w-v1-bin-Linux-Ubuntu-22.04-x86_64.zip",
    ];
    let fake = serve(&names, &zip_of(&["widget-server", "libwidget.so"])).await;

    let run = run(&fake, &spec(AssetChoice::First), target(&dirs, LINUX)).await;

    run.result.expect("the first match installs");
    assert_eq!(
        fake.asked()[1],
        "/download/w-v1-bin-Linux-Ubuntu-24.04-x86_64.zip"
    );
}

#[tokio::test]
async fn no_matching_asset_names_the_pattern_and_the_release() {
    let dirs = dirs();
    let fake = serve(
        &["w-v1-bin-Linux-Ubuntu-24.04-x86_64-vulkan.zip"],
        &zip_of(&[]),
    )
    .await;

    let run = run(&fake, &spec(AssetChoice::First), target(&dirs, LINUX)).await;

    assert_eq!(
        run.result.expect_err("no asset").to_string(),
        "No matching asset found for pattern '-bin-Linux-*-x86_64.zip' in release v1"
    );
}

/// An archive without a required member fails before anything is recorded,
/// and its download is still cleaned up.
#[tokio::test]
async fn an_archive_missing_a_required_member_records_nothing_and_leaves_no_archive() {
    let dirs = dirs();
    let fake = serve(
        &["w-v1-bin-Linux-Ubuntu-24.04-x86_64.zip"],
        &zip_of(&["widget-server"]),
    )
    .await;

    let run = run(&fake, &spec(AssetChoice::First), target(&dirs, LINUX)).await;

    assert_eq!(
        run.result.expect_err("no library").to_string(),
        "Failed to extract all required binaries. Found 1 of 2"
    );
    assert!(run.record.is_none());
    assert!(!dirs.downloads.exists(), "the downloads directory is left");
    assert_eq!(
        frames(&run.events).last().map(String::as_str),
        Some(phase("started", "extract").as_str())
    );
}

#[tokio::test]
async fn a_failed_download_says_so() {
    let dirs = dirs();
    let fake = serve(&["w-v1-bin-Linux-Ubuntu-24.04-x86_64.zip"], &[]).await;
    fake.route(
        "/download/w-v1-bin-Linux-Ubuntu-24.04-x86_64.zip",
        Reply {
            status: 500,
            body: Vec::new(),
        },
    );

    let run = run(&fake, &spec(AssetChoice::First), target(&dirs, LINUX)).await;

    assert_eq!(
        run.result.expect_err("a server error").to_string(),
        "Download failed: HTTP 500 Internal Server Error"
    );
}

#[test]
fn a_matcher_needs_every_part_and_the_ending() {
    let m = AssetMatcher {
        contains: &["-bin-win-", "cuda12"],
        ends_with: Some("-x64.zip"),
    };
    assert!(m.matches("sd-x-bin-win-cuda12-x64.zip"));
    assert!(!m.matches("sd-x-bin-win-vulkan-x64.zip"));
    assert!(!m.matches("sd-x-bin-win-cuda12-x64.zip.sig"));
    assert_eq!(m.to_string(), "-bin-win-*cuda12*-x64.zip");

    let plain = AssetMatcher {
        contains: &["bin-macos-arm64.tar.gz"],
        ends_with: None,
    };
    assert!(plain.matches("llama-b1-bin-macos-arm64.tar.gz"));
    assert_eq!(plain.to_string(), "bin-macos-arm64.tar.gz");
}

#[test]
fn an_override_floats_names_a_tag_or_falls_back_to_the_pin() {
    let spec = spec(AssetChoice::First);
    assert_eq!(selector_from_override(&spec, " "), pinned());
    assert_eq!(
        selector_from_override(&spec, "Latest"),
        ReleaseSelector::Latest
    );
    assert_eq!(
        selector_from_override(&spec, " v2 "),
        ReleaseSelector::Tag("v2".to_owned())
    );
}
