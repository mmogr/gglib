use std::path::PathBuf;

use serde::Serialize;

use super::*;

#[test]
fn a_model_out_of_memory_says_a_send_loads_it() {
    assert_eq!(
        send_line("Qwen3-4B", Some(ModelAbsence::NotLoaded), EndpointState::On).as_deref(),
        Some("Qwen3-4B isn't loaded · sending loads it")
    );
    assert_eq!(
        send_line(
            "Qwen3-4B",
            Some(ModelAbsence::Crashed),
            EndpointState::Pinned
        )
        .as_deref(),
        Some("Qwen3-4B stopped unexpectedly · sending loads it again")
    );
}

#[test]
fn an_endpoint_off_says_a_send_turns_it_on() {
    assert_eq!(
        send_line("Qwen3-4B", None, EndpointState::Off).as_deref(),
        Some("The endpoint is Off · sending turns it on")
    );
    assert_eq!(
        send_line(
            "Qwen3-4B",
            Some(ModelAbsence::NotLoaded),
            EndpointState::Off
        )
        .as_deref(),
        Some("Qwen3-4B isn't loaded · sending turns the endpoint on and loads it")
    );
    assert_eq!(
        send_line("Qwen3-4B", Some(ModelAbsence::Crashed), EndpointState::Off).as_deref(),
        Some("Qwen3-4B stopped unexpectedly · sending turns the endpoint on and loads it again")
    );
}

#[test]
fn a_resident_model_behind_a_listening_endpoint_says_nothing() {
    for endpoint in [
        EndpointState::On,
        EndpointState::Pinned,
        EndpointState::Starting,
    ] {
        assert_eq!(send_line("Qwen3-4B", None, endpoint), None, "{endpoint:?}");
    }
}

/// One recorded line: what the chat knows, and what it says.
#[derive(Serialize)]
struct Recorded {
    model: &'static str,
    absence: Option<ModelAbsence>,
    endpoint: EndpointState,
    line: Option<String>,
}

fn recorded() -> Vec<Recorded> {
    let absences = std::iter::once(None).chain(ModelAbsence::ALL.into_iter().map(Some));
    absences
        .flat_map(|absence| {
            EndpointState::ALL
                .into_iter()
                .map(move |endpoint| Recorded {
                    model: "Qwen3-4B",
                    absence,
                    endpoint,
                    line: send_line("Qwen3-4B", absence, endpoint),
                })
        })
        .collect()
}

fn recorded_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../contracts/vocabulary/send_line.json")
}

/// The checked-in file is every line this module words, for every absence
/// and endpoint state. Run with `GGLIB_RECORD_CONTRACTS=1` to rewrite it
/// after a deliberate change.
#[test]
fn every_line_is_recorded() {
    let mut want = serde_json::to_string_pretty(&recorded()).expect("serialise");
    want.push('\n');
    let path = recorded_path();
    if std::env::var_os("GGLIB_RECORD_CONTRACTS").is_some() {
        std::fs::write(&path, &want).expect("write send_line.json");
    }
    let have = std::fs::read_to_string(&path).expect("read contracts/vocabulary/send_line.json");
    assert!(
        have == want,
        "contracts/vocabulary/send_line.json is stale; rerun with GGLIB_RECORD_CONTRACTS=1\n{want}"
    );
}
