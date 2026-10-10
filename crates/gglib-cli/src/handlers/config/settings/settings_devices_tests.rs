//! What `gglib config settings show` prints of the device roster.

use gglib_core::{Device, Settings};

use super::{MASKED_VALUE, settings_display_rows};

/// A whole endpoint id, sixty-four hex characters.
const ENDPOINT: &str = "3ca82708b9953ca82708b9953ca82708b9953ca82708b9953ca82708b9953ca8";

/// The longest run of hex digits in `text`.
fn longest_hex_run(text: &str) -> usize {
    text.split(|c: char| !c.is_ascii_hexdigit())
        .map(str::len)
        .max()
        .unwrap_or(0)
}

/// The endpoint a device's key is pinned to is held, not printed: this output
/// is pasted into bug reports. The fingerprint beside it still is.
#[test]
fn settings_show_prints_no_whole_endpoint_id() {
    let settings = Settings {
        remote_devices: Some(vec![Device {
            id: "dev-0a1b2c3d".to_owned(),
            label: None,
            joined_at: 1,
            redeemed_at: Some(2),
            last_seen: None,
            peer: Some(ENDPOINT[..12].to_owned()),
            endpoint: Some(ENDPOINT.to_owned()),
        }]),
        ..Settings::default()
    };

    let rows = settings_display_rows(&settings, None, None);

    for (key, value) in &rows {
        assert!(
            longest_hex_run(value) < 64,
            "{key} prints a whole id: {value}"
        );
    }
    let (_, devices) = rows
        .iter()
        .find(|(k, _)| k == "remote-devices")
        .expect("the roster is listed");
    assert!(devices.contains(MASKED_VALUE), "{devices}");
    assert!(devices.contains(&ENDPOINT[..12]), "{devices}");
}
