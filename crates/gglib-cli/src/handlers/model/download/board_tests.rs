//! What the board draws for a snapshot.

use gglib_core::download::{DownloadId, DownloadPhase, FilePlace, RowFacts, row};

use super::*;

const GIB: u64 = 1024 * 1024 * 1024;

/// A running row of `repo`, a quarter in, on `place`.
fn running(repo: &str, place: Option<FilePlace>) -> DownloadRow {
    let id = DownloadId::new(repo, Some("Q8_0"));
    row(&RowFacts {
        phase: DownloadPhase::Downloading,
        bytes: 7 * GIB,
        speed_bps: Some(118_400_000.0),
        eta_seconds: Some(160.0),
        ..RowFacts::waiting(&id, 1, place, Some(28 * GIB))
    })
}

fn waiting(repo: &str, position: u32) -> DownloadRow {
    let id = DownloadId::new(repo, Some("Q8_0"));
    row(&RowFacts::waiting(&id, position, None, Some(5 * GIB)))
}

fn snapshot(active: Option<DownloadRow>, waiting: Vec<DownloadRow>) -> QueueSnapshot {
    QueueSnapshot {
        active,
        waiting,
        ..QueueSnapshot::default()
    }
}

fn ids(board: &DownloadBoard) -> Vec<&str> {
    let mut ids: Vec<&str> = board.bars.keys().map(String::as_str).collect();
    ids.sort_unstable();
    ids
}

/// A download keeps its one bar as it moves from file to file, and loses it
/// when it leaves the queue.
#[test]
fn sync_keeps_one_bar_per_row() {
    let mut board = DownloadBoard::new(Arc::new(CliConsole::hidden()));
    let part = |number| Some(FilePlace::Part { number, of: 3 });

    board.sync_bars(&snapshot(
        Some(running("o/a", part(1))),
        vec![waiting("o/b", 2)],
    ));
    assert_eq!(ids(&board), ["o/a:Q8_0", "o/b:Q8_0"]);
    let first = board.bars["o/a:Q8_0"].clone();
    assert_eq!(first.prefix(), "o/a:Q8_0 · part 1/3");

    board.sync_bars(&snapshot(
        Some(running("o/a", part(2))),
        vec![waiting("o/b", 2)],
    ));
    assert_eq!(ids(&board), ["o/a:Q8_0", "o/b:Q8_0"]);
    assert_eq!(
        first.prefix(),
        "o/a:Q8_0 · part 2/3",
        "the same bar, moved on"
    );
    assert_eq!((first.position(), first.length()), (250, Some(1000)));

    board.sync_bars(&snapshot(Some(running("o/b", None)), vec![]));
    assert_eq!(ids(&board), ["o/b:Q8_0"]);
    assert!(first.is_finished(), "a row that has gone takes its bar");

    board.clear();
    assert!(board.bars.is_empty());
}

/// A download fetched without a queue is drawn as a queue's running download
/// is: one bar under its id, made of the row's text, and the same bar from
/// one file to the next.
#[test]
fn a_lone_row_is_drawn_as_the_queue_draws_it() {
    let solo = SoloBoard::new(Arc::new(CliConsole::unseen()));
    let rows = solo.rows();
    let part = |number| running("o/a", Some(FilePlace::Part { number, of: 2 }));

    rows(&part(1));
    let first = solo.0.lock().unwrap().bars["o/a:Q8_0"].clone();
    assert_eq!(first.prefix(), "o/a:Q8_0 · part 1/2");
    rows(&part(2));

    let mut queue = DownloadBoard::new(Arc::new(CliConsole::unseen()));
    queue.sync(&snapshot(Some(part(2)), vec![]));
    let queued = &queue.bars["o/a:Q8_0"];
    assert_eq!(ids(&solo.0.lock().unwrap()), ["o/a:Q8_0"]);
    assert_eq!(first.prefix(), "o/a:Q8_0 · part 2/2", "the same bar");
    assert_eq!(
        (first.prefix(), first.message(), first.position()),
        (queued.prefix(), queued.message(), queued.position())
    );
    assert_eq!(first.position(), 250);

    solo.clear();
    assert!(first.is_finished());
    assert!(solo.0.lock().unwrap().bars.is_empty());
}

/// A download fetched without a queue is on the board from the first row it
/// hands over until it is over, and then its bar is gone, and what it came
/// to is handed back.
#[tokio::test]
async fn a_lone_download_is_on_the_board_until_it_is_over() {
    let solo = SoloBoard::new(Arc::new(CliConsole::unseen()));

    let board = &solo.0;
    let (bar, came_to) = solo
        .during(|rows| async move {
            assert!(board.lock().unwrap().bars.is_empty());
            rows(&running("o/a", None));
            let bar = board.lock().unwrap().bars["o/a:Q8_0"].clone();
            assert_eq!(bar.prefix(), "o/a:Q8_0");
            assert!(!bar.is_finished(), "drawn while it is fetched");
            (bar, "what the fetch came to")
        })
        .await;

    assert_eq!(came_to, "what the fetch came to");
    assert!(bar.is_finished(), "taken off once it is over");
    assert!(solo.0.lock().unwrap().bars.is_empty());
}

/// A healthy transfer is its numbers; anything else leads with its status.
#[test]
fn a_bar_is_made_of_the_rows_text() {
    let transferring = bar_parts(&running("o/a", Some(FilePlace::Projector)));
    assert_eq!(
        transferring,
        BarParts {
            prefix: "o/a:Q8_0 · projector".to_string(),
            position: 250,
            message: "25.0% · 7.00 GiB / 28.00 GiB · 118.4 MB/s · ETA 2m 40s".to_string(),
        }
    );

    let queued = bar_parts(&waiting("o/b", 2));
    assert_eq!(queued.prefix, "o/b:Q8_0");
    assert_eq!(
        (queued.position, queued.message.as_str()),
        (0, "Queued · 5.00 GiB")
    );

    let id = DownloadId::new("o/a", Some("Q8_0"));
    let facts = RowFacts {
        phase: DownloadPhase::Registering,
        bytes: GIB,
        ..RowFacts::waiting(&id, 1, None, Some(GIB))
    };
    let registering = bar_parts(&row(&facts));
    assert_eq!(registering.position, 1000);
    assert_eq!(
        registering.message,
        "Registering… · 100.0% · 1.00 GiB / 1.00 GiB"
    );

    let noted = RowFacts {
        phase: DownloadPhase::Downloading,
        notice: Some("using direct transfer…"),
        ..RowFacts::waiting(&id, 1, None, None)
    };
    let noted = bar_parts(&row(&noted));
    assert_eq!(noted.position, 0, "no size, so an empty bar");
    assert_eq!(noted.message, "using direct transfer… · 0 B · — · ETA —");
}

/// The fill is the row's percentage in tenths, rounded down, so a row that
/// counts more bytes than its size is a full bar and no more.
#[test]
fn the_fill_is_the_rows_percentage() {
    let id = DownloadId::new("o/a", Some("Q8_0"));
    let at = |bytes| {
        let facts = RowFacts {
            phase: DownloadPhase::Downloading,
            bytes,
            ..RowFacts::waiting(&id, 1, None, Some(3 * GIB))
        };
        bar_parts(&row(&facts)).position
    };

    assert_eq!(at(GIB), 333);
    assert_eq!(at(3 * GIB - 1), 999, "not full a byte short");
    assert_eq!(at(3 * GIB), 1000);
    assert_eq!(at(4 * GIB), 1000);

    // The row's own number, not one worked out again from its bytes.
    let mut disagreeing = running("o/a", None);
    disagreeing.percent = Some(40.0);
    assert_eq!(bar_parts(&disagreeing).position, 400);
}

/// Piped, a row is one line and a snapshot is a line per row.
#[test]
fn plain_line_is_one_line_per_row() {
    let snapshot = snapshot(
        Some(running("o/a", Some(FilePlace::Weights))),
        vec![waiting("o/b", 2), waiting("o/c", 3)],
    );

    let lines: Vec<String> = snapshot.rows().map(plain_line).collect();

    assert_eq!(
        lines,
        [
            "o/a:Q8_0 · weights  25.0% · 7.00 GiB / 28.00 GiB · 118.4 MB/s · ETA 2m 40s",
            "o/b:Q8_0  Queued · 5.00 GiB",
            "o/c:Q8_0  Queued · 5.00 GiB",
        ]
    );
    assert!(lines.iter().all(|line| !line.contains('\n')));
}

/// Without a terminal the board draws no bars: `sync` prints lines.
#[test]
fn a_board_that_cannot_draw_keeps_no_bars() {
    let mut board = DownloadBoard::new(Arc::new(CliConsole::hidden()));

    board.sync(&snapshot(Some(running("o/a", None)), vec![]));

    assert!(board.bars.is_empty());
    assert!(board.printed.is_some());
    let printed = board.printed;
    board.sync(&snapshot(Some(running("o/a", None)), vec![]));
    assert_eq!(board.printed, printed, "not again within two seconds");
}

/// An empty queue is no lines, and does not start the two seconds.
#[test]
fn an_idle_queue_prints_nothing() {
    let mut board = DownloadBoard::new(Arc::new(CliConsole::hidden()));

    board.sync(&snapshot(None, vec![]));

    assert!(board.printed.is_none());
}

/// The line is the entry's own words behind a mark: the board words
/// nothing of an ending itself, so a text it has never seen prints as it is.
#[test]
fn an_outcome_line_is_the_entrys_text_behind_a_mark() {
    let ended = |outcome, text: &str| FinishedDownload {
        id: "o/a:Q8_0".to_string(),
        title: "o/a:Q8_0".to_string(),
        outcome,
        text: text.to_string(),
    };
    let completed = DownloadOutcome::Completed { message: None };
    let failed = DownloadOutcome::Failed {
        error: "no route".to_string(),
    };

    assert_eq!(
        outcome_line(&ended(completed, "the daemon's words")),
        "✓ the daemon's words"
    );
    assert_eq!(outcome_line(&ended(failed, "it broke")), "✗ it broke");
    assert_eq!(
        outcome_line(&ended(DownloadOutcome::Cancelled, "stopped")),
        "✗ stopped"
    );
}
