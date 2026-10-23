//! What is kept of a server's stderr and how it reads: over an in-process
//! pipe standing in for the server's stderr, and over the kept bytes alone.

use tokio::io::{AsyncWriteExt, duplex};

use super::*;

/// The text of `bytes` pushed in one chunk.
fn text_of(bytes: &[u8]) -> String {
    let mut kept = Kept::default();
    kept.push(bytes);
    kept.text()
}

#[test]
fn nothing_printed_is_no_text() {
    assert_eq!(text_of(b""), "");
    assert_eq!(text_of(b"\n  \r\n\n"), "");
}

/// Several lines are one line of text, each trimmed, blank ones dropped, a
/// control character made a space.
#[test]
fn lines_are_joined_into_one() {
    let printed = b"Error accessing directory /path/to/allowed:\r\n\n  Error: ENOENT\tstat\n";
    assert_eq!(
        text_of(printed),
        "Error accessing directory /path/to/allowed: | Error: ENOENT stat"
    );
}

/// Only the last [`TAIL_BYTES`] are kept, however much is printed and in
/// however many chunks, and the text says that earlier ones were let go.
#[test]
fn only_the_last_bytes_are_kept() {
    let mut kept = Kept::default();
    for _ in 0..100 {
        kept.push(&[b'x'; 1000]);
    }
    kept.push(b"\nthe reason");

    assert_eq!(kept.bytes.len(), TAIL_BYTES);
    let text = kept.text();
    assert!(text.starts_with('…'), "{text}");
    assert!(text.ends_with(" | the reason"), "{text}");
}

/// A cut through a multi-byte character drops that character whole: the text
/// is valid and has no replacement character in it, wherever the cut lands.
#[test]
fn a_cut_through_a_character_drops_it_whole() {
    let cycle = "é€😀";
    let mut repeats = Vec::new();
    while repeats.len() <= TAIL_BYTES {
        repeats.extend(cycle.as_bytes());
    }
    // Each byte of padding after the repeats moves the cut one byte along the
    // cycle, so across a cycle's length it lands before every byte of it:
    // on a character's first byte, and one, two and three bytes into one.
    for pad in 0..cycle.len() {
        let mut printed = repeats.clone();
        printed.extend(std::iter::repeat_n(b'a', pad));
        let text = text_of(&printed);
        assert!(!text.contains('\u{FFFD}'), "pad {pad}: {text}");
        assert!(text.starts_with('…'), "pad {pad}");
        assert!(
            text.ends_with(&format!("😀{}", "a".repeat(pad))),
            "pad {pad}: {text}"
        );
    }
}

/// A character that reorders how text displays, a bidirectional override or
/// isolate, is made a space like a control character, so a server cannot
/// rearrange the line that quotes it.
#[test]
fn a_reordering_character_is_a_space() {
    let printed = "evil \u{202E}txt.exe\u{202C} and \u{2066}isolated\u{2069}\u{200F}".as_bytes();
    assert_eq!(text_of(printed), "evil  txt.exe  and  isolated");
}

/// Bytes that are not UTF-8 at all are replaced, not refused.
#[test]
fn bytes_that_are_not_utf8_are_replaced() {
    assert_eq!(text_of(b"bad \xff byte"), "bad \u{FFFD} byte");
}

/// What a server printed is all there once its stderr ends, read before the
/// wait's bound.
#[tokio::test]
async fn the_tail_is_whole_once_the_stream_ends() {
    let (mut server, stderr) = duplex(64);
    let tail = StderrTail::drain(stderr);
    let print = tokio::spawn(async move {
        for n in 0..50 {
            server
                .write_all(format!("line {n}\n").as_bytes())
                .await
                .unwrap();
        }
    });

    let text = tail.settled(Duration::from_secs(5)).await;

    print.await.unwrap();
    assert!(text.starts_with("line 0 | line 1"), "{text}");
    assert!(text.ends_with("line 49"), "{text}");
}

/// A stream that stays open is waited on no longer than asked, and what it
/// has printed so far is the text.
#[tokio::test]
async fn a_stream_that_stays_open_is_waited_on_no_longer_than_asked() {
    let (mut server, stderr) = duplex(64);
    let tail = StderrTail::drain(stderr);
    server.write_all(b"starting\n").await.unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;

    let started = std::time::Instant::now();
    let text = tail.settled(Duration::from_millis(100)).await;

    assert!(started.elapsed() < Duration::from_secs(2));
    assert_eq!(text, "starting");
    drop(server);
}

/// A chatty server's stderr is read as it writes, so a writer that prints
/// far more than a pipe holds is never left waiting.
#[tokio::test]
async fn a_chatty_stream_never_blocks_its_writer() {
    let (mut server, stderr) = duplex(1024);
    let _tail = StderrTail::drain(stderr);

    let wrote = tokio::time::timeout(Duration::from_secs(5), async {
        for _ in 0..256 {
            server.write_all(&[b'x'; 1024]).await.unwrap();
        }
    })
    .await;

    assert!(wrote.is_ok(), "the writer blocked on a full pipe");
}

/// Read on a thread of its own, what a server printed is all there once its
/// stderr ends, as it is read by a task.
#[tokio::test]
async fn a_tail_read_on_a_thread_is_whole_once_the_stream_ends() {
    let (stderr, mut server) = std::io::pipe().unwrap();
    let tail = StderrTail::drain_on_thread(stderr).unwrap();
    let print = std::thread::spawn(move || {
        use std::io::Write as _;
        for n in 0..50 {
            writeln!(server, "line {n}").unwrap();
        }
    });

    let text = tail.settled(Duration::from_secs(5)).await;

    print.join().unwrap();
    assert!(text.starts_with("line 0 | line 1"), "{text}");
    assert!(text.ends_with("line 49"), "{text}");
}

/// Read on a thread of its own, a chatty server's stderr is read as it
/// writes, and the tail can be dropped while the writer is still open.
#[tokio::test]
async fn a_tail_read_on_a_thread_never_blocks_its_writer() {
    let (stderr, mut server) = std::io::pipe().unwrap();
    let tail = StderrTail::drain_on_thread(stderr).unwrap();

    let wrote = tokio::task::spawn_blocking(move || {
        use std::io::Write as _;
        for _ in 0..256 {
            server.write_all(&[b'x'; 1024]).unwrap();
        }
        server
    });
    let server = tokio::time::timeout(Duration::from_secs(5), wrote)
        .await
        .expect("the writer blocked on a full pipe")
        .unwrap();

    drop(tail);
    drop(server);
}
