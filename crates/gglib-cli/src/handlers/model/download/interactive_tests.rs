//! A model queued from the monitor is dropped, not queued, when it is
//! cancelled while its queueing waits.

use super::unless_cancelled;

/// A cancel that comes while the queueing waits ends it, with no answer.
#[tokio::test]
async fn a_cancel_while_queueing_waits_ends_it() {
    let answer = unless_cancelled(std::future::pending::<u32>(), async {}).await;

    assert_eq!(answer, None);
}

/// With no cancel the queueing's answer is the answer.
#[tokio::test]
async fn without_a_cancel_the_queueing_answers() {
    let answer = unless_cancelled(async { 7 }, std::future::pending()).await;

    assert_eq!(answer, Some(7));
}
