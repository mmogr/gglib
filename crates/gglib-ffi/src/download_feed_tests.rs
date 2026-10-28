//! The feed reads the manager's snapshots in order: a row is where its
//! download stands, and a download seen live ends as its finished entry
//! says.

use gglib_core::download::{DownloadId, DownloadRowText, FinishedDownload};

use super::*;

const REPO: &str = "owner/model-GGUF";

fn id() -> DownloadId {
    DownloadId::new(REPO, Some("Q4_K_M"))
}

fn row(phase: DownloadPhase, bytes: u64) -> DownloadRow {
    DownloadRow {
        id: id().to_string(),
        model_id: REPO.to_owned(),
        quantization: Some("Q4_K_M".to_owned()),
        phase,
        position: 1,
        downloaded_bytes: bytes,
        total_bytes: Some(100),
        percent: None,
        speed_bps: None,
        eta_seconds: None,
        text: DownloadRowText::default(),
    }
}

/// A snapshot with `active` running and `finished` ended.
fn snapshot(active: Option<DownloadRow>, finished: Vec<FinishedDownload>) -> AppEvent {
    AppEvent::Download {
        event: DownloadEvent::queue_snapshot(QueueSnapshot {
            active,
            finished,
            ..QueueSnapshot::default()
        }),
    }
}

fn ended(outcome: DownloadOutcome) -> FinishedDownload {
    FinishedDownload::new(&id(), outcome)
}

fn standing(feed: &DownloadFeed) -> Standing {
    feed.tracked(&id().to_string())
        .expect("the feed has heard of it")
        .standing
}

fn at(state: DownloadState, bytes: u64) -> Standing {
    Standing::At(DownloadProgress {
        id: id().to_string(),
        bytes,
        total: Some(100),
        state,
    })
}

#[test]
fn a_row_is_where_its_download_stands() {
    let feed = DownloadFeed::new();

    feed.emit(snapshot(Some(row(DownloadPhase::Queued, 0)), Vec::new()));
    assert_eq!(standing(&feed), at(DownloadState::Queued, 0));

    feed.emit(snapshot(
        Some(row(DownloadPhase::Downloading, 40)),
        Vec::new(),
    ));
    assert_eq!(standing(&feed), at(DownloadState::Running, 40));

    feed.emit(snapshot(
        Some(row(DownloadPhase::Registering, 100)),
        Vec::new(),
    ));
    assert_eq!(standing(&feed), at(DownloadState::Verifying, 100));
}

#[test]
fn a_download_seen_live_ends_as_its_finished_entry_says() {
    for (outcome, last) in [
        (
            DownloadOutcome::Completed { message: None },
            at(DownloadState::Done, 100),
        ),
        (
            DownloadOutcome::Failed {
                error: "Network error: reset".to_owned(),
            },
            at(
                DownloadState::Failed {
                    reason: "Network error: reset".to_owned(),
                },
                40,
            ),
        ),
        (DownloadOutcome::Cancelled, Standing::Cancelled),
    ] {
        let feed = DownloadFeed::new();
        feed.emit(snapshot(
            Some(row(DownloadPhase::Downloading, 40)),
            Vec::new(),
        ));

        feed.emit(snapshot(None, vec![ended(outcome)]));

        assert_eq!(standing(&feed), last);
    }
}

/// A finished entry left from an earlier attempt is not this one's ending:
/// only a download seen live ends.
#[test]
fn a_finished_entry_for_a_download_not_seen_live_is_passed_over() {
    let feed = DownloadFeed::new();

    feed.emit(snapshot(None, vec![ended(DownloadOutcome::Cancelled)]));

    assert!(feed.tracked(&id().to_string()).is_none());
}

#[test]
fn a_paused_download_ends_paused_and_any_other_cancelled_one_ends() {
    let feed = DownloadFeed::new();
    let running = row(DownloadPhase::Downloading, 40);
    feed.emit(snapshot(Some(running.clone()), Vec::new()));

    feed.pause(std::slice::from_ref(&running));
    feed.emit(snapshot(None, vec![ended(DownloadOutcome::Cancelled)]));

    assert_eq!(standing(&feed), at(DownloadState::Paused, 40));
    let paused = feed.take_paused();
    assert_eq!(paused.len(), 1);
    assert_eq!(
        (paused[0].repo.as_str(), paused[0].quantization.as_deref()),
        (REPO, Some("Q4_K_M"))
    );
    assert!(feed.take_paused().is_empty(), "taken once");
}

/// A pause too late to stop a download, which ends done or failed, leaves
/// nothing for a resume to queue again.
#[test]
fn a_paused_download_that_ends_done_or_failed_is_no_longer_paused() {
    for (outcome, state) in [
        (
            DownloadOutcome::Completed { message: None },
            DownloadState::Done,
        ),
        (
            DownloadOutcome::Failed {
                error: "Integrity".to_owned(),
            },
            DownloadState::Failed {
                reason: "Integrity".to_owned(),
            },
        ),
    ] {
        let feed = DownloadFeed::new();
        let finishing = row(DownloadPhase::Registering, 100);
        feed.emit(snapshot(Some(finishing.clone()), Vec::new()));

        feed.pause(std::slice::from_ref(&finishing));
        feed.emit(snapshot(None, vec![ended(outcome)]));

        assert_eq!(standing(&feed), at(state, 100));
        assert!(feed.take_paused().is_empty());
    }
}

/// A download that ends done or failed between the snapshot a pause read
/// and the pause being noted is not queued again by a resume either.
#[test]
fn a_download_that_ended_before_its_pause_was_noted_is_not_resumed() {
    for (outcome, state, bytes) in [
        (
            DownloadOutcome::Completed { message: None },
            DownloadState::Done,
            100,
        ),
        (
            DownloadOutcome::Failed {
                error: "Integrity".to_owned(),
            },
            DownloadState::Failed {
                reason: "Integrity".to_owned(),
            },
            40,
        ),
    ] {
        let feed = DownloadFeed::new();
        let running = row(DownloadPhase::Downloading, 40);
        feed.emit(snapshot(Some(running.clone()), Vec::new()));
        feed.emit(snapshot(None, vec![ended(outcome)]));

        feed.pause(std::slice::from_ref(&running));

        assert!(feed.take_paused().is_empty());
        assert_eq!(standing(&feed), at(state, bytes));
    }
}

/// A download cancelled through its watch before its pause was noted
/// stays cancelled through a resume.
#[test]
fn a_download_cancelled_before_its_pause_was_noted_is_not_resumed() {
    let feed = DownloadFeed::new();
    let running = row(DownloadPhase::Downloading, 40);
    feed.emit(snapshot(Some(running.clone()), Vec::new()));
    feed.emit(snapshot(None, vec![ended(DownloadOutcome::Cancelled)]));

    feed.pause(std::slice::from_ref(&running));

    assert!(feed.take_paused().is_empty());
    assert_eq!(standing(&feed), Standing::Cancelled);
}

/// A paused download the feed never read a snapshot of is still queued
/// again by a resume.
#[test]
fn a_paused_download_the_feed_never_heard_of_is_resumed() {
    let feed = DownloadFeed::new();

    feed.pause(&[row(DownloadPhase::Queued, 0)]);

    assert_eq!(feed.take_paused().len(), 1);
}

/// A paused download still running is left paused by a resume, so the
/// cancellation it ends with still reads as a pause, and the next resume
/// takes it.
#[test]
fn a_paused_download_still_winding_down_is_taken_once_it_has_stopped() {
    let feed = DownloadFeed::new();
    let running = row(DownloadPhase::Downloading, 40);
    feed.emit(snapshot(Some(running.clone()), Vec::new()));
    feed.pause(std::slice::from_ref(&running));

    assert!(feed.take_paused().is_empty(), "still running");
    feed.emit(snapshot(None, vec![ended(DownloadOutcome::Cancelled)]));

    assert_eq!(standing(&feed), at(DownloadState::Paused, 40));
    assert_eq!(feed.take_paused().len(), 1);
}

#[test]
fn every_change_is_counted_and_a_snapshot_that_changes_nothing_is_not() {
    let feed = DownloadFeed::new();
    let before = feed.count();

    feed.emit(snapshot(
        Some(row(DownloadPhase::Downloading, 40)),
        Vec::new(),
    ));
    let after_one = feed.count();
    feed.emit(snapshot(
        Some(row(DownloadPhase::Downloading, 40)),
        Vec::new(),
    ));

    assert!(after_one > before);
    assert_eq!(feed.count(), after_one);
}

#[test]
fn a_download_that_cannot_be_queued_again_fails_with_the_reason() {
    let feed = DownloadFeed::new();
    feed.emit(snapshot(
        Some(row(DownloadPhase::Downloading, 40)),
        Vec::new(),
    ));

    feed.fail(&id().to_string(), "no network".to_owned());

    assert_eq!(
        standing(&feed),
        at(
            DownloadState::Failed {
                reason: "no network".to_owned()
            },
            40
        )
    );
}
