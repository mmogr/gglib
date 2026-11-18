//! A marker that opens its picture: where the copy goes, what the link
//! says, and which terminals get one.

use std::time::{Duration, SystemTime};

use gglib_core::domain::attachment::{AttachmentId, AttachmentInfo};

use super::*;
use crate::bootstrap::test_context;
use crate::handlers::agent_chat::images::images_tests::png;
use crate::handlers::agent_chat::images::markers;

/// A terminal's environment, as [`opens_links`] reads one.
fn env(set: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
    let set: Vec<(String, String)> = set
        .iter()
        .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
        .collect();
    move |name| {
        set.iter()
            .find(|(named, _)| named == name)
            .map(|(_, value)| value.clone())
    }
}

/// A folder for copies whose path has a space in it, as the data root's
/// does on macOS ("Application Support").
fn spaced(dir: &tempfile::TempDir) -> PathBuf {
    let folder = dir.path().join("Application Support").join("pictures");
    fs::create_dir_all(&folder).unwrap();
    folder
}

/// The file `address` names.
fn file_of(address: &str) -> PathBuf {
    reqwest::Url::parse(address)
        .unwrap()
        .to_file_path()
        .unwrap()
}

/// `path` last changed `days` ago.
fn aged(path: &Path, days: u64) {
    let then = SystemTime::now() - Duration::from_hours(days * 24);
    let file = fs::File::options().write(true).open(path).unwrap();
    file.set_modified(then).unwrap();
}

#[test]
fn a_local_terminal_gets_links_and_one_that_would_not_open_them_does_not() {
    let xterm = ("TERM", "xterm-256color");
    for (set, opens) in [
        (vec![xterm], true),
        (vec![xterm, ("TERM_PROGRAM", "iTerm.app")], true),
        (vec![("TERM", "xterm-ghostty")], true),
        // The file is here and the click is on another machine.
        (
            vec![xterm, ("SSH_CONNECTION", "10.0.0.2 50000 10.0.0.1 22")],
            false,
        ),
        (vec![xterm, ("SSH_TTY", "/dev/ttys003")], false),
        // A multiplexer, by either sign of one.
        (vec![xterm, ("TMUX", "/tmp/tmux-501/default,1,0")], false),
        (vec![("TERM", "tmux-256color")], false),
        (vec![("TERM", "screen")], false),
        (vec![xterm, ("STY", "1234.pts-0.host")], false),
        (
            vec![("TERM", "eterm-color"), ("INSIDE_EMACS", "29.1,term:0.96")],
            false,
        ),
        (vec![("TERM", "dumb")], false),
        (vec![("TERM", "linux")], false),
        (vec![("TERM", "")], false),
        // No TERM, as on Windows: only Windows Terminal.
        (vec![], false),
        (vec![("WT_SESSION", "a-guid")], true),
        // Set and empty is not set.
        (vec![xterm, ("SSH_TTY", "")], true),
    ] {
        assert_eq!(opens_links(env(&set)), opens, "{set:?}");
    }
}

#[test]
fn an_address_is_a_file_url_of_plain_characters_whatever_the_path_holds() {
    let dir = tempfile::tempdir().unwrap();
    let path = spaced(&dir).join("naïve café.png");

    let address = address(&path).expect("an address");

    assert!(address.starts_with("file:///"), "{address}");
    assert!(address.contains("Application%20Support"), "{address}");
    assert!(
        address.bytes().all(|byte| (33..=126).contains(&byte)),
        "{address}"
    );
    assert_eq!(file_of(&address), path);
}

#[test]
fn a_path_too_long_for_a_link_or_not_absolute_has_no_address() {
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(address(&dir.path().join("a".repeat(LONGEST_ADDRESS))), None);
    assert_eq!(address(Path::new("pictures/abc.png")), None);
}

#[test]
fn a_link_wraps_its_text_and_closes_after_it() {
    assert_eq!(
        link("file:///p/a.png", "[image 4x2 abcd1234]"),
        "\x1b]8;;file:///p/a.png\x1b\\[image 4x2 abcd1234]\x1b]8;;\x1b\\"
    );
}

/// A picture's marker links to a copy of its stored bytes, under its whole
/// id, in the folder; printed again, it links to the same copy, kept on.
#[tokio::test]
async fn a_marker_links_to_a_copy_of_the_stored_picture() {
    let dir = tempfile::tempdir().unwrap();
    let ctx = test_context(dir.path()).await;
    let service = ctx.app.attachments();
    let bytes = png(4, 2);
    let image = service.ingest(&bytes).await.unwrap().info;
    let folder = spaced(&dir);
    let pictures = Pictures::in_folder(service, &folder);

    let links = pictures.links(std::slice::from_ref(&image)).await;

    let copy = folder.join(format!("{}.png", image.id.as_str()));
    let address = links.of(&image.id).expect("a link");
    assert_eq!(file_of(address), copy);
    assert_eq!(fs::read(&copy).unwrap(), bytes);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let mode = fs::metadata(&copy).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }
    assert_eq!(fs::read_dir(&folder).unwrap().count(), 1, "no part is left");
    let short = &image.id.as_str()[..8];
    assert_eq!(
        markers(std::slice::from_ref(&image), &links),
        format!(" \x1b]8;;{address}\x1b\\[image 4x2 {short}]\x1b]8;;\x1b\\")
    );

    aged(&copy, 6);
    let again = pictures.links(std::slice::from_ref(&image)).await;
    assert_eq!(again.of(&image.id), Some(address));
    sweep(&folder, SystemTime::now() + Duration::from_hours(2 * 24));
    assert!(copy.is_file(), "linked again, so kept from then");
}

/// A picture that is not in the store keeps its plain marker, and nothing
/// is written for it; nor is anything where markers are not links.
#[tokio::test]
async fn a_picture_that_cannot_be_copied_keeps_its_plain_marker() {
    let dir = tempfile::tempdir().unwrap();
    let ctx = test_context(dir.path()).await;
    let service = ctx.app.attachments();
    let folder = spaced(&dir);
    let gone = AttachmentInfo {
        id: AttachmentId::of(b"never stored"),
        mime: "image/png".to_owned(),
        width: 4,
        height: 2,
    };
    let stored = service.ingest(&png(8, 8)).await.unwrap().info;

    let links = Pictures::in_folder(service, &folder)
        .links(&[gone.clone(), stored.clone()])
        .await;
    assert_eq!(links.of(&gone.id), None);
    assert!(
        links.of(&stored.id).is_some(),
        "the next one is still linked"
    );
    assert_eq!(
        markers(std::slice::from_ref(&gone), &links),
        format!(" [image 4x2 {}]", &gone.id.as_str()[..8])
    );

    fs::remove_file(file_of(links.of(&stored.id).unwrap())).unwrap();
    let plain = Pictures::plain(service)
        .links(std::slice::from_ref(&stored))
        .await;
    assert_eq!(plain.of(&stored.id), None);
    assert_eq!(fs::read_dir(&folder).unwrap().count(), 0);
}

#[test]
fn a_sweep_removes_copies_past_a_week_and_keeps_the_rest() {
    let dir = tempfile::tempdir().unwrap();
    let (old, new) = (dir.path().join("old.png"), dir.path().join("new.png"));
    for (path, days) in [(&old, 8), (&new, 6)] {
        fs::write(path, b"png").unwrap();
        aged(path, days);
    }

    sweep(dir.path(), SystemTime::now());

    assert!(!old.exists());
    assert!(new.is_file());
}

/// A copy that cannot be finished leaves nothing behind: here its name is
/// taken by a folder, so the rename fails.
#[tokio::test]
async fn a_copy_that_cannot_be_finished_leaves_no_part() {
    let dir = tempfile::tempdir().unwrap();
    let ctx = test_context(dir.path()).await;
    let service = ctx.app.attachments();
    let image = service.ingest(&png(4, 2)).await.unwrap().info;
    let folder = spaced(&dir);
    let taken = folder.join(format!("{}.png", image.id.as_str()));
    fs::create_dir(&taken).unwrap();
    fs::write(taken.join("kept"), b"").unwrap();

    let links = Pictures::in_folder(service, &folder)
        .links(std::slice::from_ref(&image))
        .await;

    assert_eq!(links.of(&image.id), None);
    let left: Vec<_> = fs::read_dir(&folder).unwrap().flatten().collect();
    assert_eq!(left.len(), 1, "{left:?}");
    assert!(taken.is_dir());
}

/// On a terminal that draws pictures, a stored picture is drawn two
/// columns in, sized by its own pixels; one that is not in the store, and
/// any on a terminal that draws none, is not.
#[tokio::test]
async fn a_stored_picture_is_drawn_where_the_terminal_draws_them() {
    use base64::Engine as _;
    use base64::engine::general_purpose::STANDARD;

    let dir = tempfile::tempdir().unwrap();
    let ctx = test_context(dir.path()).await;
    let service = ctx.app.attachments();
    let bytes = png(16, 8);
    let image = service.ingest(&bytes).await.unwrap().info;
    let gone = AttachmentInfo {
        id: AttachmentId::of(b"never stored"),
        ..image.clone()
    };
    let folder = spaced(&dir);
    let both = [gone, image];

    // Whatever window the tests run in: a wide one, and none.
    let kitty = Pictures::in_folder(service, &folder).drawing(Inline::Kitty);
    let drawn = kitty.drawn_in(&both, Some(120)).await;
    assert_eq!(kitty.drawn_in(&both, None).await, drawn);
    // One column narrower than the picture's two, less the margin.
    let narrow = kitty.drawn_in(&both, Some(3)).await;
    assert!(narrow.contains(",c=1,"), "{narrow:?}");

    // 16 pixels are two columns.
    let encoded = STANDARD.encode(&bytes);
    assert_eq!(
        drawn,
        format!("  \x1b_Ga=T,f=100,q=2,c=2,m=0;{encoded}\x1b\\\n")
    );
    assert_eq!(Pictures::in_folder(service, &folder).drawn(&both).await, "");
    assert_eq!(Pictures::plain(service).drawn(&both).await, "");
}
