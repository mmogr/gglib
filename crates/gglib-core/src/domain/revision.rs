//! The revision a model's files came from, as the library stores it and
//! as a person reads it.
//!
//! A download records a commit sha when it knows one (an upgrade's), and
//! otherwise the ref it asked the Hub for under a `rev:` mark, `main` when
//! none was named (`gglib-download`'s worker). Both sit in one column, so
//! a reader that shortened every value to a sha's first characters showed
//! `rev:mai`. This type tells them apart once, for the CLI and the GUI.

use serde::{Deserialize, Serialize};

/// What a stored revision is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum RevisionKind {
    /// A commit, forty hex characters.
    Sha,
    /// A branch or tag name, as the Hub was asked for it.
    Ref,
}

/// A model's revision: its kind, its value, and the form a person reads.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct Revision {
    pub kind: RevisionKind,
    /// The whole value: the sha, or the ref's name decoded.
    pub value: String,
    /// What a listing shows: a sha's first twelve characters, a ref whole.
    pub short: String,
}

/// How many of a sha's characters a listing shows: enough to tell commits
/// apart, not the whole forty.
const SHORT_SHA: usize = 12;

impl Revision {
    /// The stored value read back: `rev:<name>` is a ref (percent-decoded as
    /// the worker encoded it), forty hex characters a sha, and anything else
    /// a ref as it is.
    #[must_use]
    pub fn parse(stored: &str) -> Self {
        if let Some(encoded) = stored.strip_prefix("rev:") {
            return Self::reference(percent_decode(encoded));
        }
        if stored.len() == 40 && stored.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Self {
                kind: RevisionKind::Sha,
                value: stored.to_owned(),
                short: stored[..SHORT_SHA].to_owned(),
            };
        }
        Self::reference(stored.to_owned())
    }

    fn reference(value: String) -> Self {
        Self {
            kind: RevisionKind::Ref,
            short: value.clone(),
            value,
        }
    }
}

/// `%XX` escapes decoded; a malformed escape is kept as it is.
fn percent_decode(encoded: &str) -> String {
    let bytes = encoded.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let (Some(hi), Some(lo)) = (hex(bytes.get(i + 1)), hex(bytes.get(i + 2)))
        {
            out.push(hi * 16 + lo);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn hex(byte: Option<&u8>) -> Option<u8> {
    byte.and_then(|b| (*b as char).to_digit(16))
        .map(|d| u8::try_from(d).unwrap_or(0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_forty_hex_value_is_a_sha_shown_by_its_first_twelve_characters() {
        let revision = Revision::parse("0123456789abcdef0123456789abcdef01234567");
        assert_eq!(revision.kind, RevisionKind::Sha);
        assert_eq!(revision.short, "0123456789ab");
        assert_eq!(revision.value.len(), 40);
    }

    #[test]
    fn a_rev_mark_is_a_ref_shown_whole_and_decoded() {
        let main = Revision::parse("rev:main");
        assert_eq!(
            (main.kind, main.short.as_str()),
            (RevisionKind::Ref, "main")
        );
        let branch = Revision::parse("rev:refs%2Fpr%2F12");
        assert_eq!(branch.value, "refs/pr/12");
        assert_eq!(branch.short, "refs/pr/12");
    }

    #[test]
    fn anything_else_is_a_ref_as_it_is() {
        let odd = Revision::parse("abc123");
        assert_eq!(
            (odd.kind, odd.short.as_str()),
            (RevisionKind::Ref, "abc123")
        );
        assert_eq!(percent_decode("a%2"), "a%2", "a malformed escape stays");
    }

    #[test]
    fn the_wire_names_the_kind_in_snake_case_and_the_fields_in_camel_case() {
        let json = serde_json::to_string(&Revision::parse("rev:main")).unwrap();
        assert_eq!(json, r#"{"kind":"ref","value":"main","short":"main"}"#);
    }
}
