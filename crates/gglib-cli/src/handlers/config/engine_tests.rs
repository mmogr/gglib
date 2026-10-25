//! What the shared install and offer say for either engine, and which model
//! keeps an engine from being replaced.

use super::*;
use gglib_runtime::engine::{InstallPlan, PinStanding, PinState, UninstallOutcome};

fn state(standing: PinStanding, installed: Option<&str>) -> PinState {
    PinState {
        standing,
        installed_release: installed.map(str::to_owned),
        target_release: Some("b11429".to_owned()),
        built: installed.map(|_| false),
        rebuilds: false,
        update_downloads: false,
    }
}

fn server(name: &str, runtime: RuntimeKind) -> ServerInfo {
    ServerInfo {
        model_id: 1,
        model_name: name.to_owned(),
        pid: None,
        port: 9000,
        started_at: 0,
        runtime,
    }
}

#[test]
fn the_model_that_keeps_an_engine_is_one_its_own_server_runs() {
    let servers = [
        server("qwen", RuntimeKind::Llama),
        server("flux", RuntimeKind::StableDiffusion),
    ];
    assert_eq!(
        model_on(RuntimeKind::Llama, &servers).as_deref(),
        Some("qwen")
    );
    assert_eq!(
        model_on(RuntimeKind::StableDiffusion, &servers).as_deref(),
        Some("flux")
    );
    assert_eq!(model_on(RuntimeKind::StableDiffusion, &servers[..1]), None);
    assert_eq!(model_on(RuntimeKind::Llama, &[]), None);
}

#[test]
fn an_install_under_a_running_model_is_refused_by_name() {
    assert_eq!(
        serving_refusal(RuntimeKind::Llama, "qwen"),
        "qwen is running on llama-server. Stop it before installing llama.cpp: replacing \
         llama-server under a running server leaves it serving from files that are gone."
    );
}

#[test]
fn a_removal_under_a_running_model_is_refused_by_name() {
    assert_eq!(
        removal_refusal(RuntimeKind::Llama, "qwen"),
        "qwen is running on llama-server. Stop it before removing llama.cpp."
    );
    assert_eq!(
        removal_refusal(RuntimeKind::StableDiffusion, "flux"),
        "flux is running on sd-server. Stop it before removing stable-diffusion.cpp."
    );
}

#[test]
fn an_update_is_not_offered_under_a_running_model() {
    assert_eq!(
        not_while_serving(
            RuntimeKind::Llama,
            &state(PinStanding::Older, Some("b10327")),
            "qwen"
        ),
        "llama.cpp b10327 is older than the pinned b11429. It is not updated while qwen runs on it."
    );
}

/// What `gglib serve` has always said when the user declines an engine it
/// cannot go on without.
#[test]
fn declining_a_missing_engine_names_the_command_that_installs_it() {
    assert_eq!(
        required_refusal(RuntimeKind::Llama),
        "llama.cpp is required to run this command. Run 'gglib config llama install' manually."
    );
    assert_eq!(
        required_refusal(RuntimeKind::StableDiffusion),
        "stable-diffusion.cpp is required to serve an image model. Run 'gglib config sd \
         install' manually."
    );
}

#[test]
fn an_install_with_nothing_to_do_says_where_it_stands_and_how_to_force_one() {
    assert_eq!(
        nothing_to_do_lines(&state(PinStanding::AtPin, Some("b11429")), false),
        [
            "Pinned release: b11429, which is the one installed.",
            "Use --force to reinstall it.",
        ]
    );
    assert_eq!(
        nothing_to_do_lines(&state(PinStanding::Newer, Some("b11870")), false),
        [
            "Pinned release: b11429. The one installed, b11870, is newer.",
            "Use --force to install the pinned release.",
        ]
    );
    // `--force` alone takes the download, so a build asked for says both,
    // and so does a source build that is there.
    assert_eq!(
        nothing_to_do_lines(&state(PinStanding::AtPin, Some("b11429")), true)[1],
        "Use --build --force to reinstall it."
    );
    let built = PinState {
        built: Some(true),
        rebuilds: true,
        update_downloads: false,
        ..state(PinStanding::AtPin, Some("b11429"))
    };
    assert_eq!(
        nothing_to_do_lines(&built, false)[1],
        "Use --build --force to reinstall it."
    );
}

/// The CPU-only build is the one the platform table warns about, and the
/// warning comes before the download starts, not after it has finished.
#[test]
fn a_download_says_what_it_fetches_after_any_warning() {
    let warning = "Images will take minutes each.";
    assert_eq!(
        plan_lines(
            RuntimeKind::StableDiffusion,
            &InstallPlan::Download {
                description: "Linux x64 (CPU)".to_owned(),
                warning: Some(warning.to_owned()),
            }
        ),
        [
            format!("\u{26a0}\u{fe0f}  {warning}"),
            "Downloading the pre-built stable-diffusion.cpp for Linux x64 (CPU)...".to_owned(),
        ]
    );
    assert_eq!(
        plan_lines(
            RuntimeKind::Llama,
            &InstallPlan::Download {
                description: "macOS ARM64 (Metal)".to_owned(),
                warning: None,
            }
        ),
        ["Downloading the pre-built llama.cpp for macOS ARM64 (Metal)..."]
    );
}

#[test]
fn a_build_nobody_asked_for_says_why() {
    assert_eq!(
        plan_lines(
            RuntimeKind::StableDiffusion,
            &InstallPlan::Build {
                no_download: Some("Unsupported Linux architecture".to_owned()),
            }
        ),
        ["Unsupported Linux architecture; building sd-server from source instead."]
    );
    assert!(
        plan_lines(
            RuntimeKind::Llama,
            &InstallPlan::Build { no_download: None }
        )
        .is_empty()
    );
}

#[test]
fn an_older_engine_is_offered_the_pin_as_a_download() {
    let (lines, question) = offer(
        RuntimeKind::Llama,
        &state(PinStanding::Older, Some("b10327")),
        &InstallPlan::Download {
            description: "macOS ARM64 (Metal)".to_owned(),
            warning: None,
        },
    );

    assert_eq!(
        lines,
        [
            "",
            "\u{2139}\u{fe0f}  llama.cpp b10327 is installed; gglib now uses b11429.",
            "",
            "A pre-built llama.cpp b11429 is available for macOS ARM64 (Metal).",
            "",
        ]
    );
    assert_eq!(question, "Download it now?");
}

/// A missing engine is named with what needs it and where it will go.
#[test]
fn a_missing_engine_says_what_needs_it() {
    let (lines, _) = offer(
        RuntimeKind::StableDiffusion,
        &state(PinStanding::Missing, None),
        &InstallPlan::Download {
            description: "macOS universal (Metal)".to_owned(),
            warning: None,
        },
    );

    assert_eq!(
        lines[1],
        "\u{26a0}\u{fe0f}  stable-diffusion.cpp is not installed; image models need its sd-server."
    );
    assert!(lines[2].starts_with("   Server path: "), "{lines:?}");
}

/// A source build of a release is built again, and says what that costs
/// before it asks.
#[test]
fn a_kept_source_build_is_told_it_builds_again_and_what_it_costs() {
    let (lines, question) = offer(
        RuntimeKind::Llama,
        &state(PinStanding::Older, Some("b10327")),
        &InstallPlan::Build { no_download: None },
    );

    assert_eq!(
        lines,
        [
            "",
            "\u{2139}\u{fe0f}  llama.cpp b10327 is installed; gglib now uses b11429.",
            "",
            "llama.cpp was built from source here, so it is built from source again.",
            "   This compiles llama.cpp for your hardware and can take 15-30 minutes.",
            "   It happens once; later runs reuse the binaries.",
            "",
        ]
    );
    assert_eq!(question, "Build it now?");
}

/// The tools a build needs are listed, and how to install them is the text
/// `config llama install` gives when one is missing.
#[test]
fn a_machine_without_a_download_is_told_why_and_what_a_build_needs() {
    let (lines, question) = offer(
        RuntimeKind::Llama,
        &state(PinStanding::Older, Some("b10327")),
        &InstallPlan::Build {
            no_download: Some("Unsupported Linux architecture".to_owned()),
        },
    );

    let mut expected: Vec<String> = [
        "",
        "\u{2139}\u{fe0f}  llama.cpp b10327 is installed; gglib now uses b11429.",
        "",
        "Unsupported Linux architecture",
        "",
        "llama.cpp will be built from source for this machine.",
        "   This compiles llama.cpp for your hardware and can take 15-30 minutes.",
        "   It happens once; later runs reuse the binaries.",
        "",
        "Required build tools:",
        "  • git - for cloning the repository",
        "  • cmake - for build configuration",
        "  • g++ or clang++ - for compilation",
        "",
    ]
    .map(str::to_owned)
    .to_vec();
    expected.extend(build_tool_install_lines());
    expected.push(String::new());

    assert_eq!(lines, expected);
    assert_eq!(question, "Build it now?");
}

#[test]
fn uninstall_says_each_path_it_removed() {
    let outcome = UninstallOutcome {
        was_installed: true,
        removed_paths: vec!["/data/.sd/bin".to_owned()],
    };
    assert_eq!(
        uninstall_lines(RuntimeKind::StableDiffusion, &outcome),
        [
            "\u{2713} Removed /data/.sd/bin",
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
        uninstall_lines(RuntimeKind::Llama, &outcome),
        ["llama.cpp is not installed."]
    );
}
