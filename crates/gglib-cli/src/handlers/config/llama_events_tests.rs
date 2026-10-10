//! What each command leaves on the terminal for a scripted run of events.
//!
//! The spinners and bars are drawn on stderr and taken away again, so what is
//! asserted is what is left: the lines written to `out`.

use super::*;
use gglib_core::domain::RuntimeKind;

/// Feed `events` to the build renderer as a finished build would have, and
/// return the lines it wrote.
async fn build_lines(
    events: Vec<BuildEvent>,
    fetching: Option<&'static str>,
    ending: BuildEnding,
) -> Vec<String> {
    let (tx, rx) = mpsc::channel(64);
    for event in events {
        tx.send(event).await.unwrap();
    }
    drop(tx);

    let mut out = Vec::new();
    render_build_events(rx, fetching, ending, &mut out).await;
    lines_of(&out)
}

async fn download_lines(events: Vec<LlamaProgressEvent>, ending: DownloadEnding) -> Vec<String> {
    let (tx, rx) = mpsc::channel(64);
    for event in events {
        tx.send(event).await.unwrap();
    }
    drop(tx);

    let mut out = Vec::new();
    render_install_events(rx, RuntimeKind::Llama.label(), ending, &mut out).await;
    lines_of(&out)
}

fn lines_of(out: &[u8]) -> Vec<String> {
    String::from_utf8(out.to_vec())
        .unwrap()
        .lines()
        .map(str::to_owned)
        .collect()
}

fn log(message: &str) -> BuildEvent {
    BuildEvent::Log {
        message: message.to_owned(),
    }
}

fn phase(phase: BuildPhase) -> [BuildEvent; 2] {
    [
        BuildEvent::PhaseStarted { phase },
        BuildEvent::PhaseCompleted { phase },
    ]
}

/// A source build over a checkout that is already there, as the pipeline
/// reports one: a line before any phase, a line during one, every phase, and
/// the end.
fn a_build() -> Vec<BuildEvent> {
    let mut events = vec![
        log("Using existing llama.cpp repository."),
        BuildEvent::PhaseStarted {
            phase: BuildPhase::Configure,
        },
        log("-- Configuring done"),
        BuildEvent::PhaseCompleted {
            phase: BuildPhase::Configure,
        },
        BuildEvent::PhaseStarted {
            phase: BuildPhase::Compile,
        },
        BuildEvent::Progress {
            current: 50,
            total: 100,
        },
        BuildEvent::PhaseCompleted {
            phase: BuildPhase::Compile,
        },
    ];
    events.extend(phase(BuildPhase::InstallBinaries));
    events.push(BuildEvent::Completed {
        version: "abc1234".to_owned(),
        acceleration: "Metal".to_owned(),
    });
    events
}

/// `config llama install` and `rebuild` end a build the same way. A line
/// logged between phases is left on the terminal; one logged during a phase
/// is drawn above its spinner, which is not in `out`.
#[tokio::test]
async fn an_install_and_a_rebuild_leave_the_lines_they_always_have() {
    assert_eq!(
        build_lines(a_build(), CLONING, built_and_installed).await,
        [
            "Using existing llama.cpp repository.",
            "",
            "✓ llama.cpp installed successfully!",
            "  Version:       abc1234",
            "  Acceleration:  Metal",
            "You can now use 'gglib serve', 'gglib proxy', and 'gglib chat'.",
        ]
    );
}

/// The fetch of the checkout, as the pipeline reports one: the phase, with a
/// line logged during it.
fn a_fetch(logged: &str) -> Vec<BuildEvent> {
    let [started, completed] = phase(BuildPhase::CloneOrUpdateRepo);
    vec![started, log(logged), completed]
}

/// An update is drawn as an install is but for its pull, and ends in its own
/// words. git reports a pull on the terminal itself, so no spinner is drawn
/// across it and the line that announces it is left on the terminal.
#[tokio::test]
async fn an_update_leaves_its_pull_to_git_and_ends_in_its_own_words() {
    let mut events = a_fetch("Pulling latest llama.cpp changes...");
    events.extend(a_build());

    assert_eq!(
        build_lines(events, GIT_REPORTS_THE_PULL, updated).await,
        [
            "Pulling latest llama.cpp changes...",
            "Using existing llama.cpp repository.",
            "",
            "✓ llama.cpp updated successfully!",
            "  New version: abc1234",
            "  Acceleration: Metal",
        ]
    );
}

/// A first install clones under a spinner, so what git logs of the clone is
/// drawn above it and is not in `out`. The spinner says it is a clone, as it
/// always has; a pull has none to say anything untrue of it.
#[tokio::test]
async fn a_clone_is_drawn_under_a_spinner_that_says_so_and_a_pull_under_none() {
    let cloned = a_fetch("Cloning into '/data/.llama/llama.cpp'...");
    assert!(
        build_lines(cloned, CLONING, built_and_installed)
            .await
            .is_empty()
    );

    let over_the_fetch =
        |fetching| build_indicator(BuildPhase::CloneOrUpdateRepo, fetching).map(|pb| pb.message());
    assert_eq!(
        over_the_fetch(CLONING).as_deref(),
        Some("Cloning llama.cpp repository...")
    );
    assert_eq!(over_the_fetch(GIT_REPORTS_THE_PULL), None);
}

/// The build `gglib serve` and `gglib up` run on the way to their own work
/// ends in one line, and does not tell the user to run the command they ran.
#[tokio::test]
async fn a_build_run_in_passing_ends_in_one_line() {
    assert_eq!(
        build_lines(a_build(), CLONING, built_in_passing).await,
        [
            "Using existing llama.cpp repository.",
            "✓ Build complete (abc1234)",
        ]
    );
}

/// A build that stops short has no ending: the pipeline's error is the
/// command's, and nothing here claims a success.
#[tokio::test]
async fn a_build_that_never_completes_has_no_ending() {
    let mut events = a_build();
    events.pop();

    assert_eq!(
        build_lines(events, CLONING, built_and_installed).await,
        ["Using existing llama.cpp repository."]
    );
}

fn a_download() -> Vec<LlamaProgressEvent> {
    let mut events = Vec::new();
    for phase in [
        InstallPhase::CheckAvailability,
        InstallPhase::FetchRelease,
        InstallPhase::Download,
        InstallPhase::Extract,
        InstallPhase::Verify,
    ] {
        events.push(LlamaProgressEvent::PhaseStarted { phase });
        if phase == InstallPhase::Download {
            events.push(LlamaProgressEvent::Progress {
                downloaded: 512,
                total: 1024,
                rate_bps: Some(256.0),
                eta_seconds: Some(2.0),
            });
        }
        events.push(LlamaProgressEvent::PhaseCompleted { phase });
    }
    events.push(LlamaProgressEvent::Completed {
        version: "b10327".to_owned(),
    });
    events
}

#[tokio::test]
async fn a_downloaded_install_leaves_the_lines_it_always_has() {
    // The ending names where the server is, so fix where that is first.
    gglib_core::paths::isolate_data_root();
    let server = llama_server_path().unwrap();

    assert_eq!(
        download_lines(a_download(), downloaded_and_installed).await,
        [
            "",
            "✓ llama.cpp installed successfully!",
            "  Version: b10327",
            &format!("  Server:  {}", server.display()),
            "",
            "You can now use 'gglib serve', 'gglib proxy', and 'gglib chat'.",
        ]
    );
}

#[tokio::test]
async fn a_download_run_in_passing_ends_in_one_line() {
    assert_eq!(
        download_lines(a_download(), downloaded_in_passing).await,
        ["✓ llama.cpp installed (b10327)"]
    );
}

/// The download's bar names the product being downloaded, and every other
/// phase reads the same for both: llama.cpp's words unchanged, and
/// stable-diffusion.cpp's own name on its download.
#[test]
fn a_download_is_labelled_with_its_product() {
    let sd = RuntimeKind::StableDiffusion.label();
    let llama = RuntimeKind::Llama.label();
    assert_eq!(
        install_indicator(InstallPhase::Download, sd).message(),
        "Downloading stable-diffusion.cpp binaries..."
    );
    assert_eq!(
        install_indicator(InstallPhase::Download, llama).message(),
        "Downloading llama.cpp binaries..."
    );
    assert_eq!(
        install_indicator(InstallPhase::Extract, sd).message(),
        "Extracting binaries and libraries..."
    );
}
