//! FNV-1a 32-bit hash utility used across the `gglib-agent` crate.
//!
//! Timing utilities live in `gglib_core::utils::timing` and are re-exported
//! from the `gglib_core` crate root as `gglib_core::elapsed_ms`.

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

}
