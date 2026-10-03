//! The use-don't-change table read for each machine, the wire shapes of a
//! machine and a model named by it, and what [`machine_name`] keeps of a host
//! name.

use super::{Machine, ModelAction, ModelRef, machine_name};

/// The common shapes: a bare host name, and one with its domain.
#[test]
fn the_first_label_is_the_name() {
    assert_eq!(machine_name("Desk.local").as_deref(), Some("Desk"));
    assert_eq!(machine_name("desk").as_deref(), Some("desk"));
    assert_eq!(
        machine_name("build_box-2.lan.example.com").as_deref(),
        Some("build_box-2")
    );
}

/// The label limit is the DNS one: 63 characters fit, 64 do not.
#[test]
fn a_label_is_at_most_63_characters() {
    let longest = "a".repeat(63);
    assert_eq!(machine_name(&longest), Some(longest.clone()));
    assert_eq!(machine_name(&"a".repeat(64)), None);
}

/// Nothing to show is `None`, never an empty name.
#[test]
fn an_empty_label_is_no_name() {
    assert_eq!(machine_name(""), None);
    assert_eq!(machine_name(".local"), None);
}

/// Anything outside the plain host-label set is refused whole rather than
/// cleaned up: a name the other machine wrote is not ours to rewrite.
#[test]
fn a_label_with_anything_else_in_it_is_refused() {
    for raw in [
        "desk\u{1b}[31m",
        "desk\n",
        "de\0sk",
        "desk/../etc",
        "my desk",
        "bureau-é",
        "desk:8080",
    ] {
        assert_eq!(machine_name(raw), None, "{raw:?}");
    }
}

/// This machine allows every action.
#[test]
fn this_machine_allows_everything() {
    for action in ModelAction::ALL {
        assert!(Machine::Local.allows(action), "{action:?}");
    }
    assert_eq!(Machine::Local.actions(), ModelAction::ALL);
}

/// The paired machine refuses exactly what changes its library: ADR 0013's
/// use-don't-change line, read off the one table.
#[test]
fn the_paired_machine_refuses_exactly_manage() {
    let paired = Machine::Paired {
        fingerprint: "0a1b2c3d4e5f".to_owned(),
    };
    let refused: Vec<_> = ModelAction::ALL
        .into_iter()
        .filter(|action| !paired.allows(*action))
        .collect();
    assert_eq!(refused, [ModelAction::Manage]);
    assert_eq!(
        paired.actions(),
        [
            ModelAction::List,
            ModelAction::Detail,
            ModelAction::Chat,
            ModelAction::Load
        ]
    );
}

/// The wire shapes the daemon writes and the web reads: a machine tagged by
/// `kind`, actions as lower-camel words, a ref as its machine and its id.
#[test]
fn the_shapes_on_the_wire() {
    let paired = Machine::Paired {
        fingerprint: "0a1b2c3d4e5f".to_owned(),
    };
    assert_eq!(
        serde_json::to_value(&Machine::Local).unwrap(),
        serde_json::json!({ "kind": "local" })
    );
    assert_eq!(
        serde_json::to_value(&paired).unwrap(),
        serde_json::json!({ "kind": "paired", "fingerprint": "0a1b2c3d4e5f" })
    );
    assert_eq!(
        serde_json::to_value(paired.actions()).unwrap(),
        serde_json::json!(["list", "detail", "chat", "load"])
    );
    let model = ModelRef {
        machine: paired,
        id: 3,
    };
    let json = serde_json::json!({
        "machine": { "kind": "paired", "fingerprint": "0a1b2c3d4e5f" },
        "id": 3
    });
    assert_eq!(serde_json::to_value(&model).unwrap(), json);
    assert_eq!(serde_json::from_value::<ModelRef>(json).unwrap(), model);
}

/// The TypeScript the web reads: a union discriminated by `kind`, and an id
/// that is a `number`, never a `bigint`.
#[cfg(feature = "ts-bindings")]
#[test]
fn the_typescript_shapes() {
    use ts_rs::TS;
    let config = ts_rs::Config::default();
    let machine = Machine::inline(&config);
    assert!(machine.contains(r#""kind": "local""#), "{machine}");
    assert!(machine.contains(r#""kind": "paired""#), "{machine}");
    let model = ModelRef::decl(&config);
    assert!(model.contains("id: number"), "{model}");
}
