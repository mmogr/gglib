//! Tests for [`super`] — clearing a stored endpoint key that holds no key.

use super::*;

/// The state a crash between the open and the bytes leaves behind, on every
/// modelpipe gglib has shipped against so far. modelpipe 0.7 refuses it
/// permanently rather than minting over it, and nothing in gglib deletes the
/// file — so without this the desktop could never arm again.
#[test]
fn an_empty_stored_endpoint_key_is_discarded() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let path = dir.path().join("remote_identity");
    std::fs::write(&path, b"").expect("an empty file, as a half-finished write leaves");

    discard_empty_identity(&path).expect("an empty key is discarded, not reported");

    assert!(
        !path.exists(),
        "an empty file holds no key, so it is removed and a new one minted"
    );
}

/// modelpipe's refusal is `trim().is_empty()`, not "zero bytes", so a file
/// holding only a newline gets the same "it is empty" sentence — and has to
/// be healed by the same rule, or the heal covers most of the state it
/// exists for and the operator is stuck on the rest.
#[test]
fn a_stored_endpoint_key_holding_only_whitespace_is_discarded_too() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let path = dir.path().join("remote_identity");
    std::fs::write(&path, b"\n").expect("a file holding one newline");

    discard_empty_identity(&path).expect("whitespace holds no key either");

    assert!(
        !path.exists(),
        "modelpipe would refuse this as empty, so gglib has to clear it as empty"
    );
}

/// The case that must NOT be healed. Bytes that do not parse may still be the
/// key this machine is paired under, and deleting them would unpair every
/// device to make one enable succeed. modelpipe refuses it and that refusal
/// is the operator's to act on, so this leaves the file exactly where it is.
#[test]
fn a_stored_endpoint_key_with_bytes_in_it_is_left_alone() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let path = dir.path().join("remote_identity");
    std::fs::write(&path, b"not base32 at all").expect("a file with bytes in it");

    discard_empty_identity(&path).expect("a non-empty key is not this function's business");

    assert_eq!(
        std::fs::read(&path).expect("the file is still there"),
        b"not base32 at all",
        "an unreadable key may be one devices are paired against"
    );
}

/// The ordinary first enable: there is no file yet, and that is not an error
/// here — `modelpipe::serve` mints one.
#[test]
fn a_missing_stored_endpoint_key_is_not_an_error() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    discard_empty_identity(&dir.path().join("remote_identity"))
        .expect("an absent key is the first-enable case");
}

/// Bytes that are not UTF-8 are wrong, not absent. modelpipe will refuse them
/// as "not base32" and that refusal is the operator's to act on, so this must
/// not treat an unreadable key as an empty one and delete it.
#[test]
fn a_stored_endpoint_key_that_is_not_utf8_is_left_alone() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let path = dir.path().join("remote_identity");
    std::fs::write(&path, [0xff, 0xfe, 0x00, 0x01]).expect("bytes that are not text");

    discard_empty_identity(&path).expect("unreadable bytes are not this function's business");

    assert!(
        path.exists(),
        "bytes that are not UTF-8 are wrong, not absent"
    );
}

/// A directory at the path is not a key file, and unlinking it is not this
/// function's business either — modelpipe's own error names the path and says
/// what it found.
///
/// Kept as documentation rather than as a pin: no single-branch mutation
/// kills it, because dropping the `is_file` guard leaves the read refusing a
/// directory the same way. It dies only when the guard goes *and* an
/// unreadable file is treated as empty.
#[test]
fn a_directory_at_the_stored_endpoint_key_path_is_left_alone() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let path = dir.path().join("remote_identity");
    std::fs::create_dir(&path).expect("a directory where the key should be");

    discard_empty_identity(&path).expect("a directory is modelpipe's to complain about");

    assert!(path.is_dir(), "a directory is not an empty key file");
}

/// The read is bounded, because this function exists for a path that may hold
/// something modelpipe never wrote. A key is 53 bytes; anything over a
/// kilobyte is left for modelpipe to refuse rather than read to find out.
#[test]
fn an_oversized_file_at_the_stored_endpoint_key_path_is_not_read() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let path = dir.path().join("remote_identity");
    // Whitespace, so only the size bound can be what stops it being discarded.
    std::fs::write(&path, vec![b' '; 4096]).expect("a large file of blanks");

    discard_empty_identity(&path).expect("an oversized file is left for modelpipe");

    assert!(
        path.exists(),
        "nothing over a kilobyte is read, so nothing over a kilobyte is healed"
    );
}
