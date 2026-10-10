//! Tests for [`super`]: the Model and Branched columns of `gglib chat history`.

use gglib_core::domain::ModelRef;

use super::*;

fn settings(name: &str, model: Option<ModelRef>) -> ConversationSettings {
    ConversationSettings {
        model_name: Some(name.to_owned()),
        model,
        ..ConversationSettings::default()
    }
}

/// A conversation that stored its model names it by its id and its
/// machine; one that stores none names it as it was saved.
#[test]
fn a_stored_model_is_shown_with_its_id_and_its_machine() {
    let here = settings(
        "qwen3",
        Some(ModelRef {
            machine: Machine::Local,
            id: 3,
        }),
    );
    assert_eq!(
        model_label(&here, None, MODEL_WIDTH),
        "qwen3 (3) on this machine"
    );
    assert_eq!(
        model_label(&settings("qwen3", None), None, MODEL_WIDTH),
        "qwen3"
    );
}

/// A conversation from a pairing this machine no longer holds names
/// neither the pairing it holds now nor a fingerprint.
#[test]
fn a_far_model_from_an_earlier_pairing_is_not_given_the_current_name() {
    let far = settings(
        "qwen3",
        Some(ModelRef {
            machine: Machine::Paired {
                fingerprint: "0123456789ab".to_owned(),
            },
            id: 7,
        }),
    );
    let now = RemotePairing {
        ticket: "not-a-ticket".to_owned(),
        api_key: "key".to_owned(),
        default_model: None,
        port: None,
        name: Some("desk".to_owned()),
    };

    let label = model_label(&far, Some(&now), MODEL_WIDTH);

    assert_eq!(label, "qwen3 (7) on another machine");
    assert!(!label.contains("0123"), "{label}");
}

/// A long name is shortened to fit the column, and where the chat resumes,
/// its id and its machine, is shown in full.
#[test]
fn a_long_name_is_shortened_and_its_id_and_machine_are_not() {
    let long = settings(
        "Qwen3-Coder-30B-A3B-Instruct-Q4_K_M",
        Some(ModelRef {
            machine: Machine::Local,
            id: 1234,
        }),
    );

    let label = model_label(&long, None, MODEL_WIDTH);

    assert_eq!(label.chars().count(), MODEL_WIDTH, "{label}");
    assert!(
        label.ends_with("\u{2026} (1234) on this machine"),
        "{label}"
    );
    assert!(label.starts_with("Qwen3"), "{label}");
}

/// A branch names the chat it was made from; a chat that is no branch, nothing.
#[test]
fn a_branch_names_the_chat_it_was_made_from() {
    assert_eq!(branched(Some(12)), "from #12");
    assert_eq!(branched(None), "");
}
