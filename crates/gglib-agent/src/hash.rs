//! Small, shared hashing and timing utilities used across the `gglib-agent` crate.

use std::time::Instant;

// =============================================================================
// FNV-1a 32-bit hash
// =============================================================================

/// FNV-1a 32-bit hash of `s` — bit-compatible with the TypeScript
/// `hashString` function for ASCII input.
///
/// Parameters:
/// - Offset basis: `2_166_136_261`
/// - Prime: `16_777_619`
/// - Wrapping 32-bit multiplication
///
/// The Rust implementation hashes UTF-8 bytes, matching the JavaScript
/// implementation's behaviour for the ASCII-dominated argument strings
/// produced by OpenAI-compatible tool calls.
pub fn fnv1a_32(s: &str) -> u32 {
    const OFFSET: u32 = 2_166_136_261;
    const PRIME: u32 = 16_777_619;
    let mut hash = OFFSET;
    for byte in s.bytes() {
        hash ^= u32::from(byte);
        hash = hash.wrapping_mul(PRIME);
    }
    hash
}

// =============================================================================
// Timing helper
// =============================================================================

/// Convert `Instant::elapsed()` to whole milliseconds, clamping to `u64::MAX`.
///
/// Used wherever `wait_ms` / `duration_ms` fields are populated so that the
/// same `u64::try_from(…).unwrap_or(u64::MAX)` boilerplate is not repeated.
#[inline]
pub fn elapsed_ms(start: Instant) -> u64 {
    u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX)
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fnv1a_32_empty_string_is_offset_basis() {
        // FNV-1a of "" is the offset basis unchanged.
        assert_eq!(fnv1a_32(""), 2_166_136_261);
    }

    #[test]
    fn fnv1a_32_matches_known_values() {
        // Cross-checked against the JavaScript hashString implementation.
        assert_eq!(fnv1a_32("hello"), 1_335_831_723);
        assert_eq!(fnv1a_32("world"), 933_488_787);
    }

    #[test]
    fn elapsed_ms_returns_small_value_immediately() {
        let start = Instant::now();
        let ms = elapsed_ms(start);
        // Should be 0 or very small — just verify it does not panic.
        assert!(
            ms < 1000,
            "elapsed_ms should be near zero for an immediate call"
        );
    }
}
