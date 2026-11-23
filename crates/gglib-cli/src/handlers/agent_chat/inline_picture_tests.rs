//! Which terminals draw a picture, what is written to draw one, and how
//! wide.

use super::*;

/// A terminal's environment, as [`Inline::of_terminal`] reads one.
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

/// Bytes that are not text, `len` of them.
fn bytes(len: usize) -> Vec<u8> {
    (0..=250u8).cycle().take(len).collect()
}

/// A PNG of `bytes`, to draw.
fn png(bytes: &[u8]) -> Picture<'_> {
    Picture {
        name: "3f9a2c1e.png",
        mime: "image/png",
        bytes,
    }
}

/// The payloads of the sequences in `drawn` that start `opens` and end
/// `closes`, in order.
fn payloads<'a>(drawn: &'a str, opens: &str, closes: &str) -> Vec<&'a str> {
    drawn
        .split(opens)
        .skip(1)
        .map(|rest| rest.split(closes).next().unwrap())
        .collect()
}

#[test]
fn a_terminal_known_to_draw_pictures_is_read_from_its_environment() {
    let xterm = ("TERM", "xterm-256color");
    let iterm = ("TERM_PROGRAM", "iTerm.app");
    let from_iterm = [("LC_TERMINAL", "iTerm2"), ("LC_TERMINAL_VERSION", "3.7.7")];
    let ssh = ("SSH_CONNECTION", "10.0.0.2 50000 10.0.0.1 22");
    for (set, inline) in [
        (
            vec![xterm, iterm, ("TERM_PROGRAM_VERSION", "3.7.7")],
            Some(Inline::ITerm),
        ),
        (
            vec![xterm, iterm, ("TERM_PROGRAM_VERSION", "3.5.0beta10")],
            Some(Inline::ITerm),
        ),
        // No version to go by.
        (vec![xterm, iterm], None),
        // A shell on another machine: only LC_TERMINAL travels.
        (
            vec![xterm, from_iterm[0], from_iterm[1], ssh],
            Some(Inline::ITerm),
        ),
        (vec![("TERM", "xterm-ghostty")], Some(Inline::Kitty)),
        (
            vec![xterm, ("TERM_PROGRAM", "ghostty")],
            Some(Inline::Kitty),
        ),
        (vec![("TERM", "xterm-kitty")], Some(Inline::Kitty)),
        (vec![("TERM", "xterm-kitty"), ssh], Some(Inline::Kitty)),
        (vec![xterm, ("TERM_PROGRAM", "Apple_Terminal")], None),
        (vec![xterm], None),
        (vec![], None),
        // What another terminal left in the environment is not believed:
        // one started from iTerm2 or kitty, here and not over SSH.
        (vec![xterm, from_iterm[0], from_iterm[1]], None),
        (vec![xterm, ("KITTY_WINDOW_ID", "1")], None),
        (
            vec![
                xterm,
                ("TERM_PROGRAM", "vscode"),
                from_iterm[0],
                from_iterm[1],
                ssh,
            ],
            None,
        ),
        (
            vec![("TERM", "xterm-kitty"), ("TERM_PROGRAM", "vscode")],
            None,
        ),
        // Ghostty and kitty started from iTerm2 are still themselves.
        (
            vec![
                xterm,
                ("TERM_PROGRAM", "ghostty"),
                from_iterm[0],
                from_iterm[1],
                ssh,
            ],
            Some(Inline::Kitty),
        ),
        (
            vec![("TERM", "xterm-kitty"), from_iterm[0], from_iterm[1], ssh],
            Some(Inline::Kitty),
        ),
        // Inside a multiplexer, whatever is outside it.
        (
            vec![
                xterm,
                iterm,
                ("TERM_PROGRAM_VERSION", "3.7.7"),
                ("TMUX", "/tmp/t,1,0"),
            ],
            None,
        ),
        (
            vec![("TERM", "screen-256color"), ("TERM_PROGRAM", "ghostty")],
            None,
        ),
        (
            vec![("TERM", "tmux-256color"), from_iterm[0], from_iterm[1], ssh],
            None,
        ),
        (vec![("TERM", "xterm-kitty"), ("STY", "1.pts-0.host")], None),
    ] {
        assert_eq!(Inline::of_terminal(env(&set)), inline, "{set:?}");
    }
}

#[test]
fn wezterm_and_an_iterm_before_3_5_are_told_by_what_they_set() {
    let xterm = ("TERM", "xterm-256color");
    let iterm = ("TERM_PROGRAM", "iTerm.app");
    let ssh = ("SSH_CONNECTION", "10.0.0.2 50000 10.0.0.1 22");
    let wezterm = ("TERM_PROGRAM", "WezTerm");
    for (set, inline) in [
        // Before multipart: the one-sequence form, back to 3.0.
        (
            vec![xterm, iterm, ("TERM_PROGRAM_VERSION", "3.4.23")],
            Some(Inline::ITermSingle),
        ),
        (
            vec![xterm, iterm, ("TERM_PROGRAM_VERSION", "3.0.0")],
            Some(Inline::ITermSingle),
        ),
        (
            vec![
                xterm,
                ("LC_TERMINAL", "iTerm2"),
                ("LC_TERMINAL_VERSION", "3.4.23"),
                ssh,
            ],
            Some(Inline::ITermSingle),
        ),
        // Before inline images, and with no version to go by.
        (vec![xterm, iterm, ("TERM_PROGRAM_VERSION", "2.1.4")], None),
        (vec![xterm, iterm, ("TERM_PROGRAM_VERSION", "")], None),
        (vec![xterm, ("LC_TERMINAL", "iTerm2"), ssh], None),
        // WezTerm, by the program it names, with or without its version.
        (
            vec![
                xterm,
                wezterm,
                ("TERM_PROGRAM_VERSION", "20240203-110809-5046fc22"),
            ],
            Some(Inline::ITermSingle),
        ),
        (vec![xterm, wezterm], Some(Inline::ITermSingle)),
        (vec![xterm, ("TERM_PROGRAM", "wezterm")], None),
        (vec![xterm, ("WEZTERM_PANE", "0")], None),
        (vec![xterm, wezterm, ("TMUX", "/tmp/t,1,0")], None),
    ] {
        assert_eq!(Inline::of_terminal(env(&set)), inline, "{set:?}");
    }
}

#[test]
fn a_version_is_compared_by_its_first_two_numbers() {
    for (version, least, is) in [
        ("3.5", (3, 5), true),
        ("3.5.0beta10", (3, 5), true),
        ("3.10.1", (3, 5), true),
        ("4.0", (3, 5), true),
        ("3.4.23", (3, 5), false),
        ("2.9", (3, 5), false),
        ("3", (3, 5), false),
        ("", (3, 5), false),
        ("beta", (3, 5), false),
    ] {
        assert_eq!(at_least(version, least), is, "{version}");
    }
}

#[test]
fn a_picture_is_drawn_no_wider_than_the_window_its_pixels_or_twenty_rows() {
    for (width, height, window, columns_wide) in [
        (1024, 1024, Some(120), 40),
        (1024, 1024, None, 40),
        // A narrow window, less a margin.
        (1024, 1024, Some(30), 28),
        // A small picture is not blown up: 64 pixels are 8 columns.
        (64, 32, Some(120), 8),
        // A tall one stops at twenty rows: 40 columns' worth of height.
        (768, 1344, Some(120), 22),
        (4000, 100, Some(120), 40),
        // Never nothing, whatever the numbers.
        (1, 1, Some(120), 1),
        (1024, 1024, Some(1), 1),
        // A window that was never told its size is one of unknown width.
        (1024, 1024, Some(0), 40),
        (10, 0, None, 2),
        (0, 0, None, 1),
        (u32::MAX, 1, None, 40),
    ] {
        assert_eq!(
            columns(width, height, window),
            columns_wide,
            "{width}x{height}"
        );
    }
}

#[test]
fn iterm_is_sent_a_small_picture_as_one_part_between_its_start_and_end() {
    let drawn = Inline::ITerm.drawn(&png(b"png"), 12).unwrap();

    assert_eq!(
        drawn,
        "\x1b]1337;MultipartFile=inline=1;size=3;name=M2Y5YTJjMWUucG5n;\
         width=12;preserveAspectRatio=1\x07\
         \x1b]1337;FilePart=cG5n\x07\
         \x1b]1337;FileEnd\x07\n"
    );
}

#[test]
fn iterm_is_sent_a_large_picture_in_parts_that_join_to_its_bytes() {
    let picture = bytes(10_000);

    let drawn = Inline::ITerm.drawn(&png(&picture), 40).unwrap();

    let parts = payloads(&drawn, "\x1b]1337;FilePart=", "\x07");
    assert_eq!(parts.len(), 4, "13,336 characters in parts of 4,096");
    assert!(parts.iter().all(|part| part.len() <= PART));
    assert_eq!(STANDARD.decode(parts.concat()).unwrap(), picture);
    assert!(drawn.starts_with("\x1b]1337;MultipartFile=inline=1;size=10000;"));
    assert!(drawn.ends_with("\x07\x1b]1337;FileEnd\x07\n"));
    // A JPEG is iTerm2's to decode too.
    let jpeg = Picture {
        mime: "image/jpeg",
        ..png(&picture)
    };
    assert!(Inline::ITerm.drawn(&jpeg, 40).is_some());
}

#[test]
fn wezterm_and_an_older_iterm_are_sent_a_small_picture_in_one_sequence() {
    let drawn = Inline::ITermSingle.drawn(&png(b"png"), 12).unwrap();

    assert_eq!(
        drawn,
        "\x1b]1337;File=inline=1;size=3;name=M2Y5YTJjMWUucG5n;\
         width=12;preserveAspectRatio=1:cG5n\x07\n"
    );
}

#[test]
fn wezterm_and_an_older_iterm_are_sent_a_large_picture_whole_in_one_sequence() {
    let picture = bytes(10_000);

    let drawn = Inline::ITermSingle.drawn(&png(&picture), 40).unwrap();

    let sequences = payloads(&drawn, "\x1b]1337;File=", "\x07");
    assert_eq!(sequences.len(), 1);
    let (arguments, data) = sequences[0].split_once(':').unwrap();
    assert_eq!(
        arguments,
        "inline=1;size=10000;name=M2Y5YTJjMWUucG5n;width=40;preserveAspectRatio=1"
    );
    assert_eq!(STANDARD.decode(data).unwrap(), picture);
    assert!(drawn.ends_with("\x07\n"));
    let jpeg = Picture {
        mime: "image/jpeg",
        ..png(&picture)
    };
    assert!(Inline::ITermSingle.drawn(&jpeg, 40).is_some());
}

#[test]
fn kitty_is_sent_a_small_png_whole_and_asked_to_answer_nothing() {
    let drawn = Inline::Kitty.drawn(&png(b"png"), 12).unwrap();

    assert_eq!(drawn, "\x1b_Ga=T,f=100,q=2,c=12,m=0;cG5n\x1b\\\n");
}

#[test]
fn kitty_is_sent_a_large_png_in_chunks_only_the_first_with_its_keys() {
    let picture = bytes(10_000);

    let drawn = Inline::Kitty.drawn(&png(&picture), 40).unwrap();

    let sequences = payloads(&drawn, "\x1b_G", "\x1b\\");
    let (keys, chunks): (Vec<&str>, Vec<&str>) = sequences
        .iter()
        .map(|sequence| sequence.split_once(';').unwrap())
        .unzip();
    assert_eq!(keys, ["a=T,f=100,q=2,c=40,m=1", "m=1", "m=1", "m=0"]);
    let (last, rest) = chunks.split_last().unwrap();
    assert!(rest.iter().all(|chunk| chunk.len() == PART));
    assert!(last.len() <= PART);
    assert_eq!(STANDARD.decode(chunks.concat()).unwrap(), picture);
    assert!(drawn.ends_with("\x1b\\\n"));
}

#[test]
fn kitty_is_not_sent_a_jpeg() {
    let jpeg = Picture {
        mime: "image/jpeg",
        ..png(b"jpeg")
    };

    assert_eq!(Inline::Kitty.drawn(&jpeg, 40), None);
}

#[test]
fn a_picture_with_no_bytes_is_not_sent() {
    assert_eq!(Inline::Kitty.drawn(&png(b""), 40), None);
    assert_eq!(Inline::ITerm.drawn(&png(b""), 40), None);
    assert_eq!(Inline::ITermSingle.drawn(&png(b""), 40), None);
}
