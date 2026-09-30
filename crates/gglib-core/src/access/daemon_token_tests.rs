//! Tests for the daemon token.

use super::*;

fn temp() -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!("gglib-daemon-token-{}", uuid::Uuid::new_v4()));
    p.push("daemon_token");
    p
}

/// The file is the whole protection: another account that can read it holds
/// the key to every route that changes who is trusted.
#[cfg(unix)]
#[test]
fn the_file_is_created_unreadable_to_anybody_else() {
    use std::os::unix::fs::PermissionsExt;

    let path = temp();
    load_or_mint(&path).expect("mint");

    let mode = fs::metadata(&path).expect("metadata").permissions().mode();
    assert_eq!(
        mode & 0o777,
        0o600,
        "the owner reads and writes, nobody else: {mode:o}"
    );
}

/// A restart reads the token again rather than minting one, or every client
/// holding the old one would be refused by the daemon it read it for.
#[test]
fn a_second_start_reuses_the_token() {
    let path = temp();
    let first = load_or_mint(&path).expect("first start");
    let second = load_or_mint(&path).expect("second start");

    assert_eq!(first, second);
    assert_eq!(
        read(&path).expect("read").expect("a token"),
        first,
        "a client reads the token the daemon holds"
    );
}

/// No file is no token for a client, and one mint for the daemon, whose
/// file is then what a client reads.
#[test]
fn an_absent_file_mints_once() {
    let path = temp();
    assert!(read(&path).expect("absent is not an error").is_none());

    let minted = load_or_mint(&path).expect("mint");
    let on_disk = fs::read_to_string(&path).expect("written");
    assert_eq!(on_disk, minted.as_str());
    assert_eq!(on_disk.len(), 2 * TOKEN_BYTES);
    assert!(on_disk.chars().all(|c| c.is_ascii_hexdigit()));

    load_or_mint(&path).expect("again");
    assert_eq!(
        fs::read_to_string(&path).expect("read"),
        on_disk,
        "not minted twice"
    );
}

/// A blank file is no token, and minting over it is how the daemon recovers.
#[test]
fn a_blank_file_is_minted_over() {
    let path = temp();
    fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
    fs::write(&path, " \n").expect("plant a blank file");

    assert!(read(&path).expect("read").is_none());
    let minted = load_or_mint(&path).expect("mint");
    assert_eq!(fs::read_to_string(&path).expect("read"), minted.as_str());
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
