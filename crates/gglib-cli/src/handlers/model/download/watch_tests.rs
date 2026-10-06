//! A watch draws each snapshot and says when its downloads have ended.

use std::sync::Arc;

use crate::console::CliConsole;

use super::tests::{COMPLETED, MINE, THEIRS, ended, failed, mine, running, snapshot, waiting};
use super::*;

fn console() -> Arc<CliConsole> {
    Arc::new(CliConsole::hidden())
}

/// A watch goes by the monitor's rule: an empty queue before the first row
/// is not the end, a download between two files is not, and the end comes
/// with how each download ended.
#[test]
fn a_watch_of_the_queue_ends_with_every_outcome() {
    let mut watch = QueueWatch::everything(console());

    assert_eq!(watch.take(&snapshot(None, vec![], vec![])), None);
    let busy = snapshot(Some(running(MINE)), vec![waiting(THEIRS)], vec![]);
    assert_eq!(watch.take(&busy), None);
    let last = snapshot(Some(running(THEIRS)), vec![], vec![ended(MINE, COMPLETED)]);
    assert_eq!(watch.take(&last), None);

    let outcomes = vec![ended(MINE, COMPLETED), ended(THEIRS, failed("no route"))];
    let drained = snapshot(None, vec![], outcomes.clone());
    assert_eq!(watch.take(&drained), Some(outcomes));
}

/// A watch of one download ends when that download has, whatever else the
/// queue still holds, and answers its own outcome alone.
#[test]
fn a_watch_of_a_download_ends_with_its_own_outcome() {
    let mut watch = QueueWatch::download(console(), &mine());

    let queued = snapshot(Some(running(THEIRS)), vec![waiting(MINE)], vec![]);
    assert_eq!(watch.take(&queued), None);
    let fetching = snapshot(Some(running(MINE)), vec![waiting(THEIRS)], vec![]);
    assert_eq!(watch.take(&fetching), None);

    let done = snapshot(
        Some(running(THEIRS)),
        vec![],
        vec![ended(THEIRS, failed("no route")), ended(MINE, COMPLETED)],
    );
    assert_eq!(watch.take(&done), Some(vec![ended(MINE, COMPLETED)]));
}
