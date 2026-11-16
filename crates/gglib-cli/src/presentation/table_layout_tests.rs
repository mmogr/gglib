//! A table laid out for terminals of 200, 120, 80, 60 and 40 columns, and
//! for a pipe: no line wider than the terminal, every column in line, the
//! columns that matter most kept, and a pipe given the whole table.

use super::*;

const WINDOWS_PATH: &str =
    r"C:\Users\mathi\AppData\Local\llama_models\unsloth_Qwen3-4B-GGUF\Qwen3-4B-Q4_K_M.gguf";
const LONG_NAME: &str = "DeepSeek-R1-Distill-Qwen-32B-Instruct-Abliterated-v2";
const CJK_NAME: &str = "通义千问-Qwen3-推理版";

/// A library listing's shape: an id and a name always kept, a path
/// shortened in its middle, and columns that go one by one.
fn library() -> Table {
    let mut table = Table::new(vec![
        Column::new("ID"),
        Column::new("Name").shorten(Shorten::End { min: 16 }),
        Column::new("Params").droppable(6),
        Column::new("Arch").droppable(2),
        Column::new("Quant").droppable(9),
        Column::new("Size").right().droppable(8),
        Column::new("Context").droppable(5),
        Column::new("Added").droppable(1),
        Column::new("File Path").shorten(Shorten::Path).droppable(7),
    ])
    .ruler('-');
    for (id, name, path) in [
        ("1", "Qwen3-4B", WINDOWS_PATH),
        (
            "2",
            LONG_NAME,
            "/home/me/.local/share/gglib/models/deepseek/r1-32b-abliterated-Q8_0.gguf",
        ),
        (
            "13",
            CJK_NAME,
            r"D:\模型\通义千问\qwen3-reasoning-Q5_K_M.gguf",
        ),
    ] {
        table.row(
            [
                id,
                name,
                "4.0B",
                "qwen3",
                "Q4_K_M",
                "2.33 GiB",
                "40960",
                "2026-11-16 06:58:43",
                path,
            ]
            .map(str::to_owned)
            .to_vec(),
        );
    }
    table
}

/// Each character of `line` with the display column it starts at.
fn columns_of(line: &str) -> Vec<(usize, char)> {
    let mut used = 0;
    let mut buf = [0u8; 4];
    line.chars()
        .map(|ch| {
            let at = used;
            used += display_width(ch.encode_utf8(&mut buf));
            (at, ch)
        })
        .collect()
}

/// The character starting at display column `at`, if one does.
fn char_at(line: &str, at: usize) -> Option<char> {
    columns_of(line)
        .into_iter()
        .find_map(|(start, ch)| (start == at).then_some(ch))
}

/// Each header and the display column it starts at; "File Path" is one.
fn header_starts(header: &str) -> Vec<(usize, String)> {
    let mut out: Vec<(usize, String)> = Vec::new();
    let mut previous = ' ';
    for (at, ch) in columns_of(header) {
        if ch != ' ' && previous == ' ' {
            out.push((at, String::new()));
        }
        if ch != ' ' {
            out.last_mut().expect("a header started").1.push(ch);
        }
        previous = ch;
    }
    out.retain(|(_, word)| word != "Path");
    out
}

/// Every row starts each column where its header does: a space before it
/// and a character at it. Every cell of [`library`] is filled and
/// left-aligned but Size's, which is right-aligned and so not checked by its
/// start.
fn assert_in_line(laid: &Laid, context: &str) {
    let header = laid.header.as_deref().expect("a header");
    for line in laid.rows.iter().flatten() {
        for (start, word) in header_starts(header) {
            if word == "Size" {
                continue;
            }
            let at = char_at(line, start);
            assert!(
                at.is_some_and(|ch| ch != ' '),
                "{context}: {word} starts at {start}\n{header}\n{line}"
            );
            if start > 0 {
                assert_eq!(
                    char_at(line, start - 1),
                    Some(' '),
                    "{context}: no space before {word}\n{header}\n{line}"
                );
            }
        }
    }
}

fn headers(laid: &Laid) -> String {
    laid.header.clone().unwrap_or_default()
}

#[test]
fn no_line_is_wider_than_the_terminal() {
    for width in [200, 120, 80, 60, 40] {
        let laid = library().lay_out(Some(width));
        for line in laid.lines() {
            assert!(
                display_width(&line) <= width,
                "{width} columns, a line of {}:\n{line}",
                display_width(&line)
            );
        }
    }
}

#[test]
fn the_columns_line_up_at_every_width() {
    for width in [200, 120, 80, 60, 40] {
        let laid = library().lay_out(Some(width));
        assert!(!laid.is_blocks(), "{width} columns: a table, not blocks");
        assert_in_line(&laid, &format!("{width} columns"));
    }
}

#[test]
fn the_columns_that_matter_most_survive() {
    let at = |width| headers(&library().lay_out(Some(width)));

    let wide = at(200);
    for header in [
        "ID",
        "Name",
        "Params",
        "Arch",
        "Quant",
        "Size",
        "Context",
        "Added",
        "File Path",
    ] {
        assert!(wide.contains(header), "200 columns keep {header}: {wide}");
    }
    let eighty = at(80);
    for header in ["ID", "Name", "Quant", "Size", "File Path"] {
        assert!(
            eighty.contains(header),
            "80 columns keep {header}: {eighty}"
        );
    }
    assert!(!eighty.contains("Added"), "Added goes first: {eighty}");
    let forty = at(40);
    for header in ["ID", "Name", "Quant"] {
        assert!(forty.contains(header), "40 columns keep {header}: {forty}");
    }
    assert!(!forty.contains("Added"), "{forty}");
}

/// A path keeps its file name when it is shortened.
#[test]
fn a_shortened_path_keeps_its_file_name() {
    let laid = library().lay_out(Some(80));
    let windows = &laid.rows[0][0];
    assert!(windows.ends_with(r"…\Qwen3-4B-Q4_K_M.gguf"), "{windows}");
    assert!(windows.starts_with("1 "), "{windows}");
}

/// A name is cut at its end, and a CJK name is measured by the columns it
/// takes, not its characters or bytes.
#[test]
fn a_long_name_is_cut_at_its_end_and_cjk_takes_two_columns() {
    assert_eq!(display_width(CJK_NAME), 21);
    assert_eq!(CJK_NAME.chars().count(), 14);
    let laid = library().lay_out(Some(60));
    let long = &laid.rows[1][0];
    assert!(long.contains("DeepSeek-R1-Dis"), "{long}");
    assert!(long.contains('…'), "{long}");
    assert!(!long.contains(LONG_NAME), "{long}");
}

/// Piped or redirected, nothing is shortened or dropped: every column and
/// every path whole.
#[test]
fn a_pipe_gets_the_whole_table() {
    let laid = library().lay_out(None);
    let text = laid.text();

    assert!(headers(&laid).ends_with("File Path"), "{text}");
    assert!(text.contains(WINDOWS_PATH), "{text}");
    assert!(text.contains(LONG_NAME), "{text}");
    assert!(text.contains(CJK_NAME), "{text}");
    assert!(!text.contains('…'), "{text}");
    assert_in_line(&laid, "a pipe");
}

/// The ruler is as wide as the table, and never wider than the terminal.
#[test]
fn the_ruler_is_as_wide_as_the_table() {
    for width in [None, Some(200), Some(120), Some(80), Some(40)] {
        let laid = library().lay_out(width);
        let ruler = laid.ruler.clone().expect("a ruler");
        let widest = laid
            .header
            .iter()
            .chain(laid.rows.iter().flatten())
            .map(|l| display_width(l))
            .max()
            .unwrap();
        assert_eq!(display_width(&ruler), widest, "{width:?}");
        if let Some(width) = width {
            assert!(display_width(&ruler) <= width, "{width}");
        }
    }
}

/// Too narrow for even the id and the name: one block of `label: value`
/// lines per row, every column kept, no line wider than the terminal.
#[test]
fn a_very_narrow_terminal_gets_one_block_per_row() {
    let laid = library().lay_out(Some(18));

    assert!(laid.is_blocks(), "{laid:?}");
    assert_eq!(laid.rows.len(), 3);
    for line in laid.lines() {
        assert!(display_width(&line) <= 18, "{line}");
    }
    let first = laid.rows[0].join("\n");
    assert!(first.contains("ID:"), "{first}");
    assert!(first.contains("File Path:"), "{first}");
    assert!(first.contains("Added:"), "{first}");
}

/// Prose wraps rather than losing words, and the rows stay in line.
#[test]
fn prose_wraps_onto_more_lines() {
    let mut table = Table::new(vec![
        Column::new("DEPENDENCY").pad(20),
        Column::new("NOTES").shorten(Shorten::Wrap { min: 20 }),
    ]);
    let note = "Install it with your package manager, then run gglib config check-deps again";
    table.row(vec!["cmake".to_owned(), note.to_owned()]);

    let laid = table.lay_out(Some(50));

    assert!(laid.rows[0].len() > 1, "{laid:?}");
    let notes = laid
        .header
        .as_deref()
        .and_then(|h| h.find("NOTES"))
        .unwrap();
    let joined: Vec<&str> = laid.rows[0].iter().map(|l| l[notes..].trim()).collect();
    assert_eq!(joined.join(" "), note);
    for line in laid.lines() {
        assert!(display_width(&line) <= 50, "{line}");
    }
}

/// Colour codes take no columns.
#[test]
fn colour_codes_take_no_columns() {
    let mut table = Table::new(vec![Column::new("STATUS").pad(10), Column::new("NOTES")]);
    table.row(vec!["\x1b[32m✓ found\x1b[0m".to_owned(), "x".to_owned()]);

    let laid = table.lay_out(Some(80));

    let row = &laid.rows[0][0];
    assert_eq!(console::strip_ansi_codes(row), "✓ found    x");
}

#[test]
fn a_rule_is_never_wider_than_the_terminal() {
    assert_eq!(rule('=', 60, None).chars().count(), 60);
    assert_eq!(rule('=', 60, Some(40)).chars().count(), 40);
    assert_eq!(rule('─', 40, Some(80)).chars().count(), 40);
}

#[test]
fn a_path_is_shortened_in_the_middle() {
    assert_eq!(
        shorten_path(WINDOWS_PATH, 40),
        r"C:\Users\mathi\App…\Qwen3-4B-Q4_K_M.gguf".to_owned()
    );
    assert_eq!(shorten_path("/a/b/model.gguf", 40), "/a/b/model.gguf");
    assert_eq!(shorten_path("/a/b/model.gguf", 14), "/a…/model.gguf");
    assert_eq!(shorten_path("/a/b/model.gguf", 13), "/…/model.gguf");
    // A file name wider than the room is cut in its middle, keeping its
    // start and its ending.
    assert_eq!(shorten_path("/a/b/model.gguf", 6), "mo…guf");
    assert_eq!(
        shorten_path("/m/DeepSeek-R1-Distill-Qwen-32B-Instruct-Q4_K_M.gguf", 32),
        "DeepSeek-R1-Dis…ruct-Q4_K_M.gguf"
    );
    assert!(display_width(&shorten_path(r"D:\模型\通义千问\qwen.gguf", 15)) <= 15);
}
