//! When a monitor goes on and when it exits.

use gglib_core::download::{DownloadId, DownloadPhase, DownloadRow, RowFacts, row};

use super::*;

pub(crate) const MINE: &str = "owner/mine";
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
    let id = id(repo).to_string();
    FinishedDownload {
        title: id.clone(),
        id,
        outcome,
    }
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
    let mut monitor = MonitorState::model(MINE);

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
    let mut monitor = MonitorState::model(MINE);

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

/// A repository's downloads are its own whatever the quantization, and a
/// repository whose name only starts the same is another.
#[test]
fn a_model_monitor_covers_the_repositorys_downloads() {
    let watch = Watch::Model("owner/mine".to_string());

    assert!(watch.covers("owner/mine:Q8_0"));
    assert!(watch.covers("owner/mine"));
    assert!(!watch.covers("owner/mine-too:Q8_0"));
    assert!(!watch.covers("other/mine:Q8_0"));
    assert!(Watch::Everything.covers("other/mine:Q8_0"));
}

/// The download failed before the monitor's first look: no row was ever
/// seen, and the failure is there to report.
#[test]
fn a_failure_before_the_first_poll_exits() {
    for mut monitor in [MonitorState::model(MINE), MonitorState::everything()] {
        let first = snapshot(None, vec![], vec![ended(MINE, failed("401 Unauthorized"))]);

        assert_eq!(
            monitor.step(&first),
            Step::Exit(vec![ended(MINE, failed("401 Unauthorized"))])
        );
    }
}

/// Before anything has reached the queue there is nothing to exit on.
#[test]
fn an_empty_queue_before_the_first_row_is_not_the_end() {
    for mut monitor in [MonitorState::model(MINE), MonitorState::everything()] {
        assert_eq!(
            monitor.step(&snapshot(None, vec![], vec![])),
            Step::Continue
        );
    }
}

/// Between two files a download is still the active row, and the monitor
/// stays, as it does while the download waits. It has seen its download by
/// then, so a snapshot read as having none of it would be the end.
#[test]
fn does_not_exit_between_files() {
    for mut monitor in [MonitorState::model(MINE), MonitorState::everything()] {
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
    let mut monitor = MonitorState::model(MINE);
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
    let mut monitor = MonitorState::model(MINE);
    monitor.step(&snapshot(Some(running(MINE)), vec![], vec![]));
    let refused = failed("Registration failed: Storage error: disk full");

    let Step::Exit(ended_as) = monitor.step(&snapshot(None, vec![], vec![ended(MINE, refused)]))
    else {
        panic!("the download has ended");
    };

    assert_eq!(
        failure(&ended_as).as_deref(),
        Some(
            "download failed: owner/mine:Q8_0 \u{2014} Registration failed: Storage error: disk full"
        )
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
    assert_eq!(
        failure(&cancelled).as_deref(),
        Some("download cancelled: owner/theirs:Q8_0")
    );
}

/// Watching one repository's downloads succeeds only when they completed.
/// A download that left the queue with nothing to say how is not a success.
#[test]
fn a_model_watch_succeeds_only_on_completed_downloads() {
    assert_eq!(model_result(MINE, &[ended(MINE, COMPLETED)]), Ok(()));
    assert_eq!(
        model_result(MINE, &[ended(MINE, failed("no route"))]),
        Err("download failed: owner/mine:Q8_0 \u{2014} no route".to_string())
    );
    assert_eq!(
        model_result(MINE, &[ended(MINE, DownloadOutcome::Cancelled)]),
        Err("download cancelled: owner/mine:Q8_0".to_string())
    );
    assert_eq!(
        model_result(MINE, &[]),
        Err("owner/mine left the download queue and how it ended is not recorded".to_string())
    );
}
