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
    fn anything_else_is_not() {
        for bad in ["", "a/b", "..", "a.b", "a b", "é", "a%2Fb", "a\n"] {
            assert!(!is_run_id(bad), "{bad:?}");
        }
        assert!(!is_run_id(&"z".repeat(65)));
    }
}
