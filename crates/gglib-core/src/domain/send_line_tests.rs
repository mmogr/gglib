use std::path::PathBuf;

use serde::Serialize;

use super::*;

const NOT_LOADED: Option<ModelAbsence> = Some(ModelAbsence::NotLoaded);
const CRASHED: Option<ModelAbsence> = Some(ModelAbsence::Crashed);

/// The chat's model.
const OWN: NamedModel<'static> = NamedModel {
    id: 4,
    name: "Qwen3-4B",
};
/// Another model.
const OTHER: NamedModel<'static> = NamedModel {
    id: 8,
    name: "Qwen3-8B",
};
/// Another model that shares the chat's model's name.
const NAMESAKE: NamedModel<'static> = NamedModel {
    id: 5,
    name: "Qwen3-4B",
};

fn line(
    absence: Option<ModelAbsence>,
    endpoint: EndpointState,
    pinned: Option<NamedModel<'static>>,
) -> Option<String> {
    send_line(OWN, absence, endpoint, pinned)
}

#[test]
fn a_model_out_of_memory_says_a_send_loads_it() {
    assert_eq!(
        line(NOT_LOADED, EndpointState::On, None).as_deref(),
        Some("Qwen3-4B isn't loaded · sending loads it")
    );
    assert_eq!(
        line(CRASHED, EndpointState::Starting, None).as_deref(),
        Some("Qwen3-4B stopped unexpectedly · sending loads it again")
    );
}

#[test]
fn an_endpoint_off_says_a_send_turns_it_on() {
    assert_eq!(
        line(None, EndpointState::Off, None).as_deref(),
        Some("The endpoint is Off · sending turns it on")
    );
    assert_eq!(
        line(NOT_LOADED, EndpointState::Off, None).as_deref(),
        Some("Qwen3-4B isn't loaded · sending turns the endpoint on and loads it")
    );
    assert_eq!(
        line(CRASHED, EndpointState::Off, None).as_deref(),
        Some("Qwen3-4B stopped unexpectedly · sending turns the endpoint on and loads it again")
    );
}

#[test]
fn a_resident_model_behind_a_listening_endpoint_says_nothing() {
    for endpoint in [EndpointState::On, EndpointState::Starting] {
        assert_eq!(line(None, endpoint, None), None, "{endpoint:?}");
    }
    assert_eq!(line(None, EndpointState::Pinned, Some(OWN)), None);
}

/// A pinned endpoint refuses every model but its own: the line says so,
/// whatever the chat's model is doing. Pinned to the chat's own model, a
/// send loads it as anywhere; a pin is read only from a pinned endpoint.
#[test]
fn an_endpoint_pinned_to_another_model_says_a_send_is_refused() {
    for absence in [None, NOT_LOADED, CRASHED] {
        assert_eq!(
            line(absence, EndpointState::Pinned, Some(OTHER)).as_deref(),
            Some(
                "The endpoint is Pinned to Qwen3-8B and cannot serve Qwen3-4B · sending is refused"
            ),
            "{absence:?}"
        );
    }
    assert_eq!(
        line(NOT_LOADED, EndpointState::Pinned, Some(OWN)).as_deref(),
        Some("Qwen3-4B isn't loaded · sending loads it")
    );
    assert_eq!(line(None, EndpointState::On, Some(OTHER)), None);
}

/// The pin is matched by id: pinned to another model that shares the chat's
/// model's name, a send is refused all the same, and pinned to the chat's
/// model under a name it no longer has, a send loads it.
#[test]
fn a_pin_is_matched_by_id_not_by_name() {
    assert_eq!(
        line(NOT_LOADED, EndpointState::Pinned, Some(NAMESAKE)).as_deref(),
        Some("The endpoint is Pinned to Qwen3-4B and cannot serve Qwen3-4B · sending is refused")
    );
    let renamed = NamedModel {
        id: OWN.id,
        name: "Qwen3 4B",
    };
    assert_eq!(
        line(NOT_LOADED, EndpointState::Pinned, Some(renamed)).as_deref(),
        Some("Qwen3-4B isn't loaded · sending loads it")
    );
}

/// One recorded line: what the chat knows, and what it says.
#[derive(Serialize)]
struct Recorded {
    model: NamedModel<'static>,
    absence: Option<ModelAbsence>,
    endpoint: EndpointState,
    pinned: Option<NamedModel<'static>>,
    line: Option<String>,
}

fn recorded() -> Vec<Recorded> {
    let mut all = Vec::new();
    for absence in std::iter::once(None).chain(ModelAbsence::ALL.into_iter().map(Some)) {
        for endpoint in EndpointState::ALL {
            // A pinned endpoint, pinned to the chat's model, to another, and
            // to another of the same name.
            let pins: &[Option<NamedModel<'static>>] = if endpoint == EndpointState::Pinned {
                &[Some(OWN), Some(OTHER), Some(NAMESAKE)]
            } else {
                &[None]
            };
            for &pinned in pins {
                all.push(Recorded {
                    model: OWN,
                    absence,
                    endpoint,
                    pinned,
                    line: line(absence, endpoint, pinned),
                });
            }
        }
    }
    all
}

fn recorded_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../contracts/vocabulary/send_line.json")
}

/// The checked-in file is every line this module words, for every absence
/// and endpoint state, a pinned one pinned to the chat's model, to another,
/// and to another of the same name. Run with `GGLIB_RECORD_CONTRACTS=1` to rewrite it after a
/// deliberate change.
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
