//! What [`machine_name`] keeps of a host name, and what it refuses.

use super::machine_name;

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
