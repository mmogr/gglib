//! Tests for the daemon token.

use super::*;

/// A directory of its own, removed when the guard drops, and the token
/// file's path inside it.
fn temp() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("daemon_token");
    (dir, path)
}

/// The file is the whole protection: another account that can read it holds
/// the key to every `/api` route.
#[cfg(unix)]
#[test]
fn the_file_is_created_unreadable_to_anybody_else() {
    use std::os::unix::fs::PermissionsExt;

    let (_dir, path) = temp();
    mint_and_store(&path).expect("mint");

    let mode = fs::metadata(&path).expect("metadata").permissions().mode();
    assert_eq!(
        mode & 0o777,
        0o600,
        "the owner reads and writes, nobody else: {mode:o}"
    );
}

/// A token something captured while the daemon was down is worthless after
/// the next start, and the file holds the one the daemon now asks for.
#[test]
fn every_start_mints_a_new_token() {
    let (_dir, path) = temp();
    let first = mint_and_store(&path).expect("first start");
    let second = mint_and_store(&path).expect("second start");

    assert_ne!(first, second, "a restart retires the old token");
    assert_eq!(read(&path).expect("read").expect("a token"), second);
    let on_disk = fs::read_to_string(&path).expect("written");
    assert_eq!(on_disk.len(), 2 * TOKEN_BYTES);
    assert!(on_disk.chars().all(|c| c.is_ascii_hexdigit()));
}

/// A file somebody left open to others is replaced at start, not used, and
/// what replaces it is `0600`.
#[cfg(unix)]
#[test]
fn a_loose_file_left_there_is_replaced_private() {
    use std::os::unix::fs::PermissionsExt;

    let (_dir, path) = temp();
    fs::write(&path, "planted").expect("plant");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).expect("chmod");

    let minted = mint_and_store(&path).expect("mint");

    assert_eq!(fs::read_to_string(&path).expect("read"), minted.as_str());
    let mode = fs::metadata(&path).expect("metadata").permissions().mode();
    assert_eq!(mode & 0o077, 0, "group and other have nothing: {mode:o}");
}

/// A client does not present a token others could have read, or written.
#[cfg(unix)]
#[test]
fn a_reader_refuses_a_file_open_to_others() {
    use std::os::unix::fs::PermissionsExt;

    let (_dir, path) = temp();
    mint_and_store(&path).expect("mint");
    for mode in [0o640, 0o604, 0o620, 0o602] {
        fs::set_permissions(&path, fs::Permissions::from_mode(mode)).expect("chmod");
        let refused = read(&path).expect_err("a loose file is refused");
        assert_eq!(refused.kind(), io::ErrorKind::PermissionDenied, "{mode:o}");
    }
}

#[test]
fn an_absent_or_blank_file_is_no_token() {
    let (_dir, path) = temp();
    assert!(read(&path).expect("absent is not an error").is_none());

    fs::write(&path, " \n").expect("plant a blank file");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).expect("chmod");
    }
    assert!(read(&path).expect("read").is_none());
}

#[test]
fn the_token_admits_only_itself_as_a_bearer() {
    let token = DaemonToken::mint().expect("mint");
    let other = DaemonToken::mint().expect("mint");
    assert_ne!(token, other, "two mints are two tokens");

    let good = format!("Bearer {}", token.as_str());
    assert!(token.admits(Some(&good)));
    assert!(!token.admits(Some(&format!("Bearer {}", other.as_str()))));
    assert!(!token.admits(None), "no header is no token");
    assert!(
        !token.admits(Some(token.as_str())),
        "the bare token has no scheme"
    );
    assert!(!token.admits(Some("Bearer ")));
}

/// A struct that holds the token may be logged; the token must not be.
#[test]
fn debug_hides_the_token() {
    let token = DaemonToken::mint().expect("mint");
    assert!(!format!("{token:?}").contains(token.as_str()));
}
