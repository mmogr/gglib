//! When a monitor goes on and when it exits.

use gglib_core::download::{DownloadId, DownloadPhase, DownloadRow, RowFacts, row};

use super::*;

pub(crate) const MINE: &str = "owner/mine";
/// The ID of the download of `MINE`, as the daemon answers it.
pub(crate) const MINE_ID: &str = "owner/mine:Q8_0";

pub(crate) fn mine() -> DownloadId {
    id(MINE)
}
pub(crate) const THEIRS: &str = "owner/theirs";

/// The download of `repo`, or of one quantization of it as `repo:quant`.
fn id(repo: &str) -> DownloadId {
    let (repo, quantization) = repo.split_once(':').unwrap_or((repo, "Q8_0"));
    DownloadId::new(repo, Some(quantization))
}

pub(crate) fn waiting(repo: &str) -> DownloadRow {
    row(&RowFacts::waiting(&id(repo), 2, None, None))
}

pub(crate) fn running(repo: &str) -> DownloadRow {
    let id = id(repo);
    row(&RowFacts {
        phase: DownloadPhase::Downloading,
        ..RowFacts::waiting(&id, 1, None, None)
    })
}

pub(crate) fn ended(repo: &str, outcome: DownloadOutcome) -> FinishedDownload {
    FinishedDownload::new(&id(repo), outcome)
}

pub(crate) fn failed(error: &str) -> DownloadOutcome {
    DownloadOutcome::Failed {
        error: error.to_string(),
    }
}

pub(crate) const COMPLETED: DownloadOutcome = DownloadOutcome::Completed { message: None };

pub(crate) fn snapshot(
    active: Option<DownloadRow>,
    waiting: Vec<DownloadRow>,
    finished: Vec<FinishedDownload>,
) -> QueueSnapshot {
    QueueSnapshot {
        active,
        waiting,
        finished,
        ..QueueSnapshot::default()
    }
}

/// Another download's ending is not this monitor's, and neither is another
/// download still running: it exits when its own has ended, with its own
/// outcome alone.
#[test]
fn exits_on_its_own_outcome_and_ignores_others() {
    let mut monitor = MonitorState::download(&mine());

    let theirs_failed = snapshot(
        Some(running(MINE)),
        vec![],
        vec![ended(THEIRS, failed("no route"))],
    );
    assert_eq!(monitor.step(&theirs_failed), Step::Continue);

    let mine_done = snapshot(
        Some(running(THEIRS)),
        vec![waiting("owner/mine-too")],
        vec![ended(THEIRS, failed("no route")), ended(MINE, COMPLETED)],
    );
    assert_eq!(
        monitor.step(&mine_done),
        Step::Exit(vec![ended(MINE, COMPLETED)])
    );
}

/// The daemon still lists how an earlier download of the same repository
/// ended. This run saw its own download as a row, so the older outcome is
/// neither printed nor the reason it exits.
#[test]
fn an_older_outcome_of_the_same_repository_is_not_this_runs() {
    let stale = ended("owner/mine:Q4_K_M", failed("no route"));
    let mut monitor = MonitorState::download(&mine());

    let started = snapshot(Some(running(MINE)), vec![], vec![stale.clone()]);
    assert_eq!(monitor.step(&started), Step::Continue);
    let done = snapshot(None, vec![], vec![stale.clone(), ended(MINE, COMPLETED)]);
    let Step::Exit(ended_as) = monitor.step(&done) else {
        panic!("the download has ended");
    };

    assert_eq!(ended_as, [ended(MINE, COMPLETED)]);
    assert_eq!(failure(&ended_as), None);

    // A monitor of the whole queue owns every outcome in it, seen as a row
    // or not.
    let mut everything = MonitorState::everything();
    assert_eq!(everything.step(&started), Step::Continue);
    assert_eq!(
        everything.step(&done),
        Step::Exit(vec![stale, ended(MINE, COMPLETED)])
    );
}

/// A monitor of one download covers that ID and no other, another
/// quantization of the same repository included.
#[test]
fn a_download_monitor_covers_its_own_id_alone() {
    let watch = Watch::Download(MINE_ID.to_string());

    assert!(watch.covers("owner/mine:Q8_0"));
    assert!(!watch.covers("owner/mine:Q4_K_M"));
    assert!(!watch.covers("owner/mine"));
    assert!(!watch.covers("owner/mine:Q8_0-too"));
    assert!(Watch::Everything.covers("other/mine:Q8_0"));
}

/// Its own download ended before the first look, while another
/// quantization of the same repository is still in the queue. The monitor
/// follows the ID the daemon gave its request: it exits at once with its
/// own outcome, and never with the other download's.
#[test]
fn follows_its_own_id_not_another_download_of_the_repository() {
    let other = "owner/mine:Q4_K_M";
    let mut monitor = MonitorState::download(&mine());
    let first = snapshot(Some(running(other)), vec![], vec![ended(MINE, COMPLETED)]);
    assert_eq!(
        monitor.step(&first),
        Step::Exit(vec![ended(MINE, COMPLETED)])
    );

    let mut monitor = MonitorState::download(&mine());
    let other_failed = ended(other, failed("no route"));
    let mine_running = snapshot(Some(running(MINE)), vec![], vec![other_failed.clone()]);
    assert_eq!(monitor.step(&mine_running), Step::Continue);
    let both_ended = snapshot(None, vec![], vec![other_failed, ended(MINE, COMPLETED)]);
    assert_eq!(
        monitor.step(&both_ended),
        Step::Exit(vec![ended(MINE, COMPLETED)])
    );
}

/// The download failed before the monitor's first look: no row was ever
/// seen, and the failure is there to report.
#[test]
fn a_failure_before_the_first_poll_exits() {
    for mut monitor in [MonitorState::download(&mine()), MonitorState::everything()] {
        let first = snapshot(None, vec![], vec![ended(MINE, failed("401 Unauthorized"))]);

        assert_eq!(
            monitor.step(&first),
            Step::Exit(vec![ended(MINE, failed("401 Unauthorized"))])
        );
    }
}

/// A monitor of the whole queue has nothing to exit on before anything has
/// reached it. A monitor of one download was told by the daemon that its
/// download is queued: a queue with neither a row nor an outcome of it has
/// lost how it ended, and that is the end.
#[test]
fn an_empty_queue_is_the_end_only_for_a_download_known_to_be_queued() {
    let empty = snapshot(None, vec![], vec![]);

    assert_eq!(MonitorState::everything().step(&empty), Step::Continue);
    assert_eq!(
        MonitorState::download(&mine()).step(&empty),
        Step::Exit(vec![])
    );
}

/// Between two files a download is still the active row, and the monitor
/// stays, as it does while the download waits. It has seen its download by
/// then, so a snapshot read as having none of it would be the end.
#[test]
fn does_not_exit_between_files() {
    for mut monitor in [MonitorState::download(&mine()), MonitorState::everything()] {
        let waiting = snapshot(None, vec![waiting(MINE)], vec![]);
        let between_files = snapshot(Some(running(MINE)), vec![], vec![]);

        assert_eq!(monitor.step(&waiting), Step::Continue);
        assert_eq!(monitor.step(&between_files), Step::Continue);
        assert_eq!(monitor.step(&waiting), Step::Continue);
    }
}

/// A download seen and then gone with no outcome left to read has still
/// ended: the monitor exits, with nothing to report.
#[test]
fn exits_when_its_download_is_gone_without_an_outcome() {
    let mut monitor = MonitorState::download(&mine());
    assert_eq!(
        monitor.step(&snapshot(Some(running(MINE)), vec![], vec![])),
        Step::Continue
    );

    let gone = snapshot(Some(running(THEIRS)), vec![], vec![]);

    assert_eq!(monitor.step(&gone), Step::Exit(vec![]));
}

/// The files arrived and the library refused the model. That is an outcome
/// of `Failed`, and the monitor's exit is not a success.
#[test]
fn a_registration_failure_is_not_success() {
    let mut monitor = MonitorState::download(&mine());
    monitor.step(&snapshot(Some(running(MINE)), vec![], vec![]));
    let refused = failed("Registration failed: Storage error: disk full");

    let Step::Exit(ended_as) = monitor.step(&snapshot(None, vec![], vec![ended(MINE, refused)]))
    else {
        panic!("the download has ended");
    };

    assert_eq!(
        failure(&ended_as).as_deref(),
        Some("owner/mine:Q8_0: download failed: Registration failed: Storage error: disk full")
    );
}

#[test]
fn only_a_completed_download_is_a_success() {
    let done = [ended(MINE, COMPLETED), ended(THEIRS, COMPLETED)];
    let cancelled = [
        ended(MINE, COMPLETED),
        ended(THEIRS, DownloadOutcome::Cancelled),
    ];

    assert_eq!(failure(&done), None);
    assert_eq!(failure(&[]), None);
    // The reason is the entry's text as it stands, not words made here.
    let mut reworded = ended(MINE, failed("no route"));
    reworded.text = "the daemon's words".to_string();
    assert_eq!(failure(&[reworded]).as_deref(), Some("the daemon's words"));
    assert_eq!(
        failure(&cancelled).as_deref(),
        Some("owner/theirs:Q8_0: download cancelled")
    );
}

/// Watching a download succeeds only when it completed. A download whose
/// outcome is gone from the queue is not a success.
#[test]
fn a_download_watch_succeeds_only_on_a_completed_download() {
    assert_eq!(download_result(&mine(), &[ended(MINE, COMPLETED)]), Ok(()));
    assert_eq!(
        download_result(&mine(), &[ended(MINE, failed("no route"))]),
        Err("owner/mine:Q8_0: download failed: no route".to_string())
    );
    assert_eq!(
        download_result(&mine(), &[ended(MINE, DownloadOutcome::Cancelled)]),
        Err("owner/mine:Q8_0: download cancelled".to_string())
    );
    assert_eq!(
        download_result(&mine(), &[]),
        Err(
            "owner/mine:Q8_0 left the download queue and how it ended is no longer recorded"
                .to_string()
        )
    );
}
