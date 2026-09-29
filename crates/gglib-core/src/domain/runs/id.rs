//! What a run id may be.
//!
//! The client mints the id, and it is interpolated into a request path on the
//! way back, so the charset is the whole of the check: no `/`, no `.`, no
//! escapes, nothing a client resolves into another route.

/// The longest id a run may have.
pub const RUN_ID_MAX: usize = 64;

/// Whether `id` is 1 to [`RUN_ID_MAX`] characters of `[A-Za-z0-9_-]`.
#[must_use]
pub fn is_run_id(id: &str) -> bool {
    (1..=RUN_ID_MAX).contains(&id.len())
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// A fresh id for a run this client starts: `run-` and 12 random hex digits.
#[must_use]
pub fn new_run_id() -> String {
    let hex = uuid::Uuid::new_v4().simple().to_string();
    format!("run-{}", &hex[..12])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn letters_digits_dash_and_underscore_up_to_64_are_ids() {
        assert!(is_run_id("a"));
        assert!(is_run_id("Run-01_x"));
        assert!(is_run_id(&"z".repeat(64)));
    }

    #[test]
    fn a_minted_id_is_an_id_and_two_differ() {
        let (a, b) = (new_run_id(), new_run_id());
        assert!(is_run_id(&a), "{a}");
        assert!(a.starts_with("run-") && a.len() == 16, "{a}");
        assert_ne!(a, b);
    }

    #[test]
    fn anything_else_is_not() {
        for bad in ["", "a/b", "..", "a.b", "a b", "é", "a%2Fb", "a\n"] {
            assert!(!is_run_id(bad), "{bad:?}");
        }
        assert!(!is_run_id(&"z".repeat(65)));
    }
}
