//! What the device list says for each way the roster, the key file and the
//! edge can stand, with no tunnel and no store behind them.

use gglib_core::Device;

use super::viewed;

fn row(id: &str) -> Device {
    Device {
        id: id.to_owned(),
        label: Some("a label".to_owned()),
        joined_at: 1_757_000_000_000,
        redeemed_at: Some(1_757_000_060_000),
        last_seen: None,
    }
}

fn ids(names: &[&str]) -> Vec<String> {
    names.iter().map(|name| (*name).to_owned()).collect()
}

/// #1034: a key the file holds that no roster row lists is a row of its own,
/// marked as having no record, after the roster's rows, and says whether the
/// edge is admitting it, which is the case worth a person's attention.
#[test]
fn a_key_no_row_lists_is_listed_as_held_with_no_record() {
    let both = ids(&["dev-0a1b2c3d", "dev-11112222"]);

    let view = viewed(vec![row("dev-0a1b2c3d")], &both, Some(both.as_slice()));

    let [listed, unrecorded] = view.as_slice() else {
        panic!("the roster's row, then the key without one: {view:?}");
    };
    assert!(listed.recorded);
    assert_eq!(listed.label.as_deref(), Some("a label"));
    assert_eq!(unrecorded.id, "dev-11112222");
    assert!(!unrecorded.recorded, "a key with no row says so");
    assert_eq!(
        unrecorded.admitted,
        Some(true),
        "and says whether the edge admits it"
    );
    assert_eq!(
        (
            &unrecorded.label,
            unrecorded.redeemed_at,
            unrecorded.last_seen
        ),
        (&None, None, None),
        "nothing else is known of it"
    );
}

/// A row whose key is in the file is one row, not two.
#[test]
fn a_row_and_its_key_are_one_row() {
    let view = viewed(vec![row("dev-0a1b2c3d")], &ids(&["dev-0a1b2c3d"]), None);

    assert_eq!(view.len(), 1, "{view:?}");
    assert!(view[0].recorded);
    assert_eq!(
        view[0].admitted, None,
        "nothing admits with the tunnel down"
    );
}

/// A row with no key in the file is still listed: `forget` stopping part-way
/// leaves exactly that, and the row must not vanish from the list.
#[test]
fn a_row_with_no_key_is_still_listed() {
    let view = viewed(vec![row("dev-0a1b2c3d")], &[], Some(&[][..]));

    assert_eq!(view.len(), 1, "{view:?}");
    assert_eq!(view[0].admitted, Some(false));
    assert!(view[0].recorded);
}
