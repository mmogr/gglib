//! The download's bounds: only HTTPS is followed, and a body past the cap
//! is refused as it arrives. The body is a stream of chunks made here; no
//! test reaches Microsoft.

use futures_util::stream;

use super::{CAP, MOST_HOPS, copy_capped, refused_hop, too_many_hops};

/// The permalink and its redirect targets are followed over HTTPS only: a
/// hop down to HTTP, or to anything else, is refused.
#[test]
fn only_https_is_followed() {
    let url = |text: &str| reqwest::Url::parse(text).unwrap();
    assert_eq!(
        refused_hop(&url("https://aka.ms/vc14/vc_redist.x64.exe")),
        None
    );
    assert_eq!(
        refused_hop(&url(
            "https://download.visualstudio.microsoft.com/x/VC_redist.x64.exe"
        )),
        None
    );
    for hop in [
        "http://download.visualstudio.microsoft.com/x/VC_redist.x64.exe",
        "ftp://example.com/VC_redist.x64.exe",
        "file:///C:/VC_redist.x64.exe",
    ] {
        let refused = refused_hop(&url(hop)).expect(hop);
        assert!(refused.contains("is not HTTPS"), "{refused}");
    }
}

/// A body within the cap is written whole, and its digest is the digest of
/// what was written.
#[tokio::test]
async fn a_body_within_the_cap_is_written_whole() {
    use sha2::Digest as _;
    let body = stream::iter([Ok::<_, String>(vec![1_u8; 10]), Ok(vec![2_u8; 5])]);
    let mut written = Vec::new();

    let (length, digest) = copy_capped(body, &mut written, 15).await.unwrap();

    assert_eq!(length, 15);
    assert_eq!(written.len(), 15);
    assert_eq!(digest, <[u8; 32]>::from(sha2::Sha256::digest(&written)));
}

/// Ten redirects are followed, and the eleventh is refused: `previous`
/// counts the first address as well as each redirect's.
#[test]
fn ten_redirects_are_followed_and_no_more() {
    assert_eq!(MOST_HOPS, 10);
    for previous in 1..=10 {
        assert!(!too_many_hops(previous), "redirect {previous}");
    }
    assert!(too_many_hops(11));
}

/// A body that passes the cap is refused at the chunk that passes it, and
/// nothing past the cap is written.
#[tokio::test]
async fn a_body_past_the_cap_is_refused_as_it_arrives() {
    let body = stream::iter([
        Ok::<_, String>(vec![1_u8; 10]),
        Ok(vec![2_u8; 10]),
        Ok(vec![3_u8; 10]),
    ]);
    let mut written = Vec::new();

    let refused = copy_capped(body, &mut written, 15).await.unwrap_err();

    assert!(refused.contains("larger than"), "{refused}");
    assert_eq!(written.len(), 10);
}

/// A body that fails mid-way says why.
#[tokio::test]
async fn a_body_that_fails_says_why() {
    let body = stream::iter([Ok(vec![1_u8; 3]), Err("connection reset".to_owned())]);
    let refused = copy_capped(body, &mut Vec::new(), 15).await.unwrap_err();
    assert_eq!(refused, "connection reset");
}

/// The cap is far above the real installer, about 25 MB.
#[test]
fn the_cap_leaves_room_for_the_installer() {
    assert_eq!(CAP, 200 * 1024 * 1024);
}
