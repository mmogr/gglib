//! Token pieces put together into text.
//!
//! llama.cpp hands out each token's piece as bytes, and a character can
//! span tokens: a byte-fallback vocabulary spells one emoji over four of
//! them. A piece is sent as text once the characters in it are whole.

/// The bytes of a generation's pieces, turned into text as the characters
/// in them complete.
#[derive(Debug, Default)]
pub(crate) struct Utf8Pieces {
    /// The start of a character that a later piece completes.
    held: Vec<u8>,
}

impl Utf8Pieces {
    /// `bytes` after what is held: the text whose characters are whole, with
    /// an incomplete character at the end held back for the next piece. A
    /// byte that can start no character is replaced with U+FFFD.
    pub(crate) fn push(&mut self, bytes: &[u8]) -> String {
        self.held.extend_from_slice(bytes);
        let mut text = String::new();
        let mut rest = std::mem::take(&mut self.held);
        loop {
            match std::str::from_utf8(&rest) {
                Ok(whole) => {
                    text.push_str(whole);
                    return text;
                }
                Err(e) => {
                    let (whole, tail) = rest.split_at(e.valid_up_to());
                    text.push_str(&String::from_utf8_lossy(whole));
                    match e.error_len() {
                        None => {
                            self.held = tail.to_vec();
                            return text;
                        }
                        Some(bad) => {
                            text.push(char::REPLACEMENT_CHARACTER);
                            rest = tail[bad..].to_vec();
                        }
                    }
                }
            }
        }
    }

    /// What is held when the generation ends, a character it never
    /// completed replaced with U+FFFD; empty when nothing is.
    pub(crate) fn finish(&mut self) -> String {
        let rest = String::from_utf8_lossy(&self.held).into_owned();
        self.held.clear();
        rest
    }
}

#[cfg(test)]
mod tests {
    use super::Utf8Pieces;

    #[test]
    fn whole_characters_pass_at_once() {
        let mut pieces = Utf8Pieces::default();
        assert_eq!(pieces.push(b"Once"), "Once");
        assert_eq!(pieces.push(b" upon"), " upon");
        assert_eq!(pieces.finish(), "");
    }

    #[test]
    fn a_character_split_over_pieces_waits_for_its_last_byte() {
        let smile = "🙂".as_bytes();
        let mut pieces = Utf8Pieces::default();
        assert_eq!(pieces.push(b"a"), "a");
        assert_eq!(pieces.push(&smile[..1]), "");
        assert_eq!(pieces.push(&smile[1..3]), "");
        assert_eq!(pieces.push(&[smile[3], b'!']), "🙂!");
        assert_eq!(pieces.finish(), "");
    }

    #[test]
    fn a_byte_that_starts_no_character_is_replaced_and_the_rest_kept() {
        let mut pieces = Utf8Pieces::default();
        assert_eq!(pieces.push(b"a\xffb"), "a\u{fffd}b");
        assert_eq!(pieces.push(b"\xe2\x82"), "");
        assert_eq!(pieces.push(b"x"), "\u{fffd}x");
    }

    #[test]
    fn a_character_never_completed_is_replaced_at_the_end() {
        let mut pieces = Utf8Pieces::default();
        assert_eq!(pieces.push(b"ok\xe2\x82"), "ok");
        assert_eq!(pieces.finish(), "\u{fffd}");
        assert_eq!(pieces.finish(), "");
    }
}
