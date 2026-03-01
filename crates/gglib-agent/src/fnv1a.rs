//! FNV-1a 64-bit hash utility used across the `gglib-agent` crate.
//!
//! Timing utilities live in `gglib_core::utils::timing` and are re-exported
//! from the `gglib_core` crate root as `gglib_core::elapsed_ms`.

// =============================================================================
// FNV-1a 64-bit hash
// =============================================================================

/// FNV-1a 64-bit hash of `s`.
///
/// Parameters:
/// - Offset basis: `14_695_981_039_346_656_037`
/// - Prime: `1_099_511_628_211`
/// - Wrapping 64-bit multiplication
///
/// The Rust implementation hashes UTF-8 bytes.
pub fn fnv1a_64(s: &str) -> u64 {
    const OFFSET: u64 = 14_695_981_039_346_656_037;
    const PRIME: u64 = 1_099_511_628_211;
    let mut hash = OFFSET;
    for byte in s.bytes() {
        hash ^= u64::from(byte);
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
    fn fnv1a_64_empty_string_is_offset_basis() {
        // FNV-1a of "" is the offset basis unchanged.
        assert_eq!(fnv1a_64(""), 14_695_981_039_346_656_037);
    }

    #[test]
    fn fnv1a_64_known_empty_rounds_stable() {
        // Hashing the same string twice must yield the same value.
        assert_eq!(fnv1a_64("hello"), fnv1a_64("hello"));
        assert_ne!(fnv1a_64("hello"), fnv1a_64("world"));
    }

    #[test]
    fn fnv1a_64_differs_from_32_bit_basis() {
        // Sanity check: the 64-bit offset basis is different from the 32-bit one.
        assert_ne!(fnv1a_64("") as u32, 2_166_136_261_u32);
    }
}
