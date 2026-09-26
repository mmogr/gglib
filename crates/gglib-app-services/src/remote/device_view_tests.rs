//! What the device list says for each way the roster, the key file and the
//! edge can stand, with no tunnel and no store behind them.

use gglib_core::Device;

use super::viewed;

/// The clock the rows are described at.
const NOW: i64 = 1_757_000_120_000;

fn row(id: &str) -> Device {
    Device {
        id: id.to_owned(),
        label: Some("a label".to_owned()),
        joined_at: 1_757_000_000_000,
        redeemed_at: Some(1_757_000_060_000),
        last_seen: None,
        peer: Some("3ca82708b995".to_owned()),
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

    let view = viewed(vec![row("dev-0a1b2c3d")], &both, Some(both.as_slice()), NOW);

    let [listed, unrecorded] = view.as_slice() else {
        panic!("the roster's row, then the key without one: {view:?}");
    };
    assert!(listed.recorded);
    assert_eq!(listed.label.as_deref(), Some("a label"));
    assert_eq!(listed.peer.as_deref(), Some("3ca82708b995"));
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
            unrecorded.last_seen,
            &unrecorded.peer
        ),
        (&None, None, None, &None),
        "nothing else is known of it"
    );
}

/// A row whose key is in the file is one row, not two.
#[test]
fn a_row_and_its_key_are_one_row() {
    let view = viewed(
        vec![row("dev-0a1b2c3d")],
        &ids(&["dev-0a1b2c3d"]),
        None,
        NOW,
    );

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
    let view = viewed(vec![row("dev-0a1b2c3d")], &[], Some(&[][..]), NOW);

    assert_eq!(view.len(), 1, "{view:?}");
    assert_eq!(view[0].admitted, Some(false));
    assert!(view[0].recorded);
}

/// Every row leaves here described, on the clock it was given, with whether a
/// device has arrived under it: the words both surfaces print, and the answer
/// the GUI's enable asks before it offers a code.
#[test]
fn every_row_is_described_on_the_clock_it_is_given() {
    let invited = Device {
        id: "dev-99887766".to_owned(),
        redeemed_at: None,
        peer: None,
        ..row("dev-99887766")
    };
    let held = ids(&["dev-0a1b2c3d", "dev-99887766", "dev-11112222"]);

    let view = viewed(
        vec![row("dev-0a1b2c3d"), invited],
        &held,
        Some(held.as_slice()),
        NOW,
    );

    let lines: Vec<(&str, bool)> = view
        .iter()
        .map(|d| (d.description.as_str(), d.joined))
        .collect();
    assert_eq!(
        lines,
        [
            ("no requests yet · paired from 3ca82708b995", true),
            ("invited 2m ago, never joined", false),
            ("key held, no record · admitted", false),
        ]
    );
}
