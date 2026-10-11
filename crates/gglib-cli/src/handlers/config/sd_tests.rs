//! What `config sd` says: status for each kind of install, the warning a
//! CPU-only download is given before it starts, and what an uninstall
//! removed. One test refuses an uninstall while the daemon runs an image
//! model, with `.sd/` under this binary's own data root and a stand-in on a
//! loopback port in the daemon's place; the rest read values.

use super::*;
use crate::bootstrap::test_context;
use crate::daemon_client::handle_tests::{answering, daemon_health, nobody};
use crate::daemon_client::library_changes::tests::{TOKEN, beside};
use crate::daemon_client::paths;

/// What `sd_status` reports for an install of `install_type`, or for none.
fn status(install_type: Option<&str>) -> SdStatus {
    let installed = install_type.is_some();
    SdStatus {
        installed,
        binary_path: "/data/.sd/bin/sd-server".to_owned(),
        config_path: "/data/.sd/sd-config.json".to_owned(),
        pinned_release: "master-948-228c707".to_owned(),
        install_type: install_type.map(str::to_owned),
        release: install_type.map(|_| "master-948-228c707".to_owned()),
        platform: install_type.map(|kind| {
            if kind == "source" {
                "Metal"
            } else {
                "macOS (Metal)"
            }
            .to_owned()
        }),
        installed_at: install_type.map(|_| "2026-10-10T01:02:03+00:00".to_owned()),
        record_error: None,
        version_line: install_type
            .map(|_| "stable-diffusion.cpp version unknown, commit 228c707".to_owned()),
        commit: install_type.map(|_| "228c707".to_owned()),
    }
}

#[test]
fn status_of_a_download_names_its_release_platform_commit_and_path() {
    assert_eq!(
        status_lines(&status(Some("prebuilt"))),
        [
            "Status: Installed",
            "Binary: /data/.sd/bin/sd-server",
            "",
            "Pre-built download:",
            "  Release: master-948-228c707",
            "  Platform: macOS (Metal)",
            "  Installed: 2026-10-10 01:02:03 UTC",
            "  Pinned release: master-948-228c707",
            "",
            "Binary version: stable-diffusion.cpp version unknown, commit 228c707",
            "  Commit: 228c707",
        ]
    );
}

#[test]
fn status_of_a_source_build_says_it_was_built() {
    assert_eq!(
        status_lines(&status(Some("source"))),
        [
            "Status: Installed",
            "Binary: /data/.sd/bin/sd-server",
            "",
            "Built from source:",
            "  Release: master-948-228c707",
            "  Acceleration: Metal",
            "  Built: 2026-10-10 01:02:03 UTC",
            "  Pinned release: master-948-228c707",
            "",
            "Binary version: stable-diffusion.cpp version unknown, commit 228c707",
            "  Commit: 228c707",
        ]
    );
}

#[test]
fn status_with_nothing_installed_says_the_command_that_installs_it() {
    assert_eq!(
        status_lines(&status(None)),
        [
            "Status: Not installed",
            "",
            "Run 'gglib config sd install' to install stable-diffusion.cpp (master-948-228c707).",
        ]
    );
    assert!(status_lines(&status(None))[2].contains(SD_INSTALL_COMMAND));
}

/// A binary put there by hand: no record, and a binary that does not answer.
#[test]
fn status_of_a_binary_with_no_record_and_no_version_says_so() {
    let mut bare = status(Some("prebuilt"));
    bare.install_type = None;
    bare.version_line = None;
    bare.commit = None;
    let lines = status_lines(&bare);
    assert!(
        lines.contains(&"Warning: sd-config.json not found".to_owned()),
        "{lines:#?}"
    );
    assert_eq!(
        lines.last().map(String::as_str),
        Some("Binary version: sd-server did not answer --version")
    );

    bare.record_error = Some("expected value at line 1".to_owned());
    assert!(
        status_lines(&bare).contains(
            &"Warning: Could not load sd-config.json: expected value at line 1".to_owned()
        )
    );
}

/// The CPU-only build is the one the platform table warns about, and the
/// warning comes before the download starts, not after it has finished.
#[test]
fn a_cpu_download_is_warned_about_before_it_starts() {
    let warning = "No GPU runtime gglib can use was found, so the CPU build of \
                   stable-diffusion.cpp is installed. Images will take minutes each.";
    assert_eq!(
        prebuilt_lines("Linux x64 (CPU)", Some(warning)),
        [
            format!("\u{26a0}\u{fe0f}  {warning}"),
            "Downloading the pre-built stable-diffusion.cpp for Linux x64 (CPU)...".to_owned(),
        ]
    );
}

#[test]
fn a_gpu_download_has_no_warning() {
    assert_eq!(
        prebuilt_lines("macOS (Metal)", None),
        ["Downloading the pre-built stable-diffusion.cpp for macOS (Metal)..."]
    );
}

#[test]
fn uninstall_says_each_path_it_removed() {
    let outcome = UninstallOutcome {
        was_installed: true,
        removed_paths: vec!["/data/.sd".to_owned()],
    };
    assert_eq!(
        uninstall_lines(&outcome),
        [
            "\u{2713} Removed /data/.sd",
            "stable-diffusion.cpp uninstalled successfully."
        ]
    );
}

#[test]
fn uninstall_with_nothing_removed_says_nothing_was_there() {
    let outcome = UninstallOutcome {
        was_installed: false,
        removed_paths: Vec::new(),
    };
    assert_eq!(
        uninstall_lines(&outcome),
        ["stable-diffusion.cpp is not installed."]
    );
}

#[test]
fn the_endings_name_the_product_and_what_to_do_next() {
    let built = built("master-948-228c707", "Metal");
    assert_eq!(
        built[1],
        "\u{2713} stable-diffusion.cpp installed successfully!"
    );
    assert_eq!(built[3], "  Acceleration:  Metal");
    assert_eq!(built.last().map(String::as_str), Some(NOW_USABLE));
    let downloaded = downloaded("master-948-228c707");
    assert_eq!(downloaded[2], "  Version: master-948-228c707");
    assert_eq!(downloaded.last().map(String::as_str), Some(NOW_USABLE));
}

#[test]
fn missing_build_tools_are_named_with_the_command_to_rerun() {
    let lines = missing_tools_lines(&["cmake"]);
    assert_eq!(
        lines[0],
        "Building stable-diffusion.cpp needs: cmake. Please install:"
    );
    assert_eq!(
        lines.last().map(String::as_str),
        Some("After installing, run 'gglib config sd install' again.")
    );
}

/// The daemon's `GET /api/servers` answer for `servers`, each `(name,
/// runtime)`.
fn servers(servers: &[(&str, RuntimeKind)]) -> String {
    let listed: Vec<ServerInfo> = servers
        .iter()
        .zip(1_i64..)
        .map(|(&(name, runtime), model_id)| ServerInfo {
            model_id,
            model_name: name.to_owned(),
            pid: None,
            port: 9000,
            started_at: 0,
            runtime,
        })
        .collect();
    serde_json::to_string(&listed).expect("a listing")
}

/// The daemon of this data root runs an image model: the uninstall is
/// refused by name, even with `--force`, and the `sd-server` installed under
/// this binary's data root stays. The daemon is asked twice: the probe, then
/// its servers.
#[tokio::test]
async fn an_uninstall_is_refused_while_the_daemon_runs_an_image_model() {
    let root = gglib_core::paths::isolate_data_root();
    let binary = sd_server_path().expect("a path");
    assert!(binary.starts_with(root), "{}", binary.display());
    std::fs::create_dir_all(binary.parent().expect("a bin dir")).expect("created");
    std::fs::write(&binary, b"#!/bin/sh\n").expect("written");
    let dir = tempfile::tempdir().expect("tempdir");
    let ctx = test_context(dir.path()).await;
    let listing = servers(&[
        ("qwen", RuntimeKind::Llama),
        ("flux", RuntimeKind::StableDiffusion),
    ]);
    let (port, asked) = answering(daemon_health(), listing);

    let refused = beside(port, Some(TOKEN), uninstall(&ctx, true)).await;

    assert_eq!(
        refused.expect_err("an image model runs").to_string(),
        "flux is running on sd-server. Stop it before removing stable-diffusion.cpp."
    );
    assert!(binary.exists(), "nothing was removed");
    let lines: Vec<String> = asked
        .lock()
        .unwrap()
        .iter()
        .map(|(line, _)| line.clone())
        .collect();
    assert_eq!(
        lines,
        [paths::HEALTH_PATH, paths::SERVERS_LIST_PATH].map(|path| format!("GET {path} HTTP/1.1"))
    );
}

/// Nothing is drawing with no daemon on its port (every image model is a
/// daemon's), with another program there, or with a daemon running only
/// chat models. With no token under this data root, no daemon serves it,
/// and the one on the port is asked nothing even while it draws.
#[tokio::test]
async fn nothing_is_drawing_without_this_roots_daemon_or_with_only_chat_models() {
    let dir = tempfile::tempdir().expect("tempdir");
    let ctx = test_context(dir.path()).await;
    let another_program = r#"{"service":"something-else"}"#.to_owned();
    let drawing = servers(&[("flux", RuntimeKind::StableDiffusion)]);
    let (elsewhere, asked_elsewhere) = answering(daemon_health(), drawing);

    for (what, port, token) in [
        ("no daemon", nobody(), Some(TOKEN)),
        (
            "another program",
            answering(another_program, "[]".to_owned()).0,
            Some(TOKEN),
        ),
        (
            "chat models only",
            answering(daemon_health(), servers(&[("qwen", RuntimeKind::Llama)])).0,
            Some(TOKEN),
        ),
        ("another root's daemon", elsewhere, None),
    ] {
        let drawing = beside(port, token, drawing_on_the_daemon(&ctx))
            .await
            .expect("an answer");
        assert_eq!(drawing, None, "{what}");
    }
    assert!(asked_elsewhere.lock().unwrap().is_empty());
}
