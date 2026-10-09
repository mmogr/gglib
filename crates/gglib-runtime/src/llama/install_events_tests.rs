//! The install stream's wire shape and wording, pinned as literals.
//!
//! Two surfaces read these frames (the CLI's bar and the daemon's SSE, which
//! the desktop app and a browser both parse), and the TypeScript union that
//! types them is kept by hand. A change here is a change to all of them.

use super::{InstallPhase, LlamaProgressEvent};

/// Every phase, in pipeline order.
const PHASES: [InstallPhase; 6] = [
    InstallPhase::CheckAvailability,
    InstallPhase::FetchRelease,
    InstallPhase::Download,
    InstallPhase::Extract,
    InstallPhase::CudaRuntime,
    InstallPhase::Verify,
];

fn json(event: &LlamaProgressEvent) -> String {
    serde_json::to_string(event).expect("an event serialises")
}

#[test]
fn each_phase_bracket_is_the_frame_it_has_always_been() {
    let names = [
        "check_availability",
        "fetch_release",
        "download",
        "extract",
        "cuda_runtime",
        "verify",
    ];
    for (phase, name) in PHASES.into_iter().zip(names) {
        assert_eq!(
            json(&LlamaProgressEvent::PhaseStarted { phase }),
            format!(r#"{{"type":"phase_started","phase":"{name}"}}"#)
        );
        assert_eq!(
            json(&LlamaProgressEvent::PhaseCompleted { phase }),
            format!(r#"{{"type":"phase_completed","phase":"{name}"}}"#)
        );
    }
}

#[test]
fn progress_carries_rate_and_eta_only_once_they_are_known() {
    assert_eq!(
        json(&LlamaProgressEvent::Progress {
            downloaded: 512,
            total: 1024,
            rate_bps: Some(256.5),
            eta_seconds: Some(2.0),
        }),
        r#"{"type":"progress","downloaded":512,"total":1024,"rate_bps":256.5,"eta_seconds":2.0}"#
    );
    assert_eq!(
        json(&LlamaProgressEvent::Progress {
            downloaded: 0,
            total: 0,
            rate_bps: None,
            eta_seconds: None,
        }),
        r#"{"type":"progress","downloaded":0,"total":0}"#
    );
}

#[test]
fn the_endings_are_the_frames_they_have_always_been() {
    assert_eq!(
        json(&LlamaProgressEvent::Completed {
            version: "b10327".to_owned(),
        }),
        r#"{"type":"completed","version":"b10327"}"#
    );
    assert_eq!(
        json(&LlamaProgressEvent::Failed {
            message: "no asset".to_owned(),
        }),
        r#"{"type":"failed","message":"no asset"}"#
    );
}

#[test]
fn each_phase_reads_as_it_always_has() {
    let labels = [
        "Checking platform availability...",
        "Fetching release information...",
        "Downloading llama.cpp binaries...",
        "Extracting binaries and libraries...",
        "Downloading CUDA runtime libraries...",
        "Verifying installation...",
    ];
    for (phase, label) in PHASES.into_iter().zip(labels) {
        assert_eq!(phase.label(), label, "{phase:?}");
    }
}

#[test]
fn a_phase_worded_for_llama_cpp_is_its_label_and_another_product_is_named() {
    for phase in PHASES {
        assert_eq!(phase.label_for("llama.cpp"), phase.label(), "{phase:?}");
    }
    assert_eq!(
        InstallPhase::Download.label_for("stable-diffusion.cpp"),
        "Downloading stable-diffusion.cpp binaries..."
    );
    assert_eq!(
        InstallPhase::Extract.label_for("stable-diffusion.cpp"),
        "Extracting binaries and libraries..."
    );
}
