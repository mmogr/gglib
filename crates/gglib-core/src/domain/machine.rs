//! The name a machine is shown by.
//!
//! A machine's host name reaches the other side of a pairing as text it wrote
//! about itself, so it is untrusted. [`machine_name`] keeps only a plain host
//! label: anything that is not one is dropped rather than repaired.

/// The longest DNS label, and so the longest name [`machine_name`] keeps.
const MAX_LABEL: usize = 63;

/// The display name in `raw`, a host name: its first DNS label, kept only
/// when that label is 1 to 63 ASCII letters, digits, `-` or `_`.
///
/// `Desk.local` is `Desk`. A label carrying anything else — a control
/// character, a `/`, a space, a non-ASCII letter — gives `None`, as does an
/// empty or over-long one, so a caller shows its own fallback instead of
/// text the other machine chose.
#[must_use]
pub fn machine_name(raw: &str) -> Option<String> {
    let label = raw.split('.').next().unwrap_or_default();
    let plain = (1..=MAX_LABEL).contains(&label.len())
        && label
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
    plain.then(|| label.to_owned())
}

#[cfg(test)]
#[path = "machine_tests.rs"]
mod machine_tests;
