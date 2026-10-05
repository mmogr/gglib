//! Tests of [`FileCounter`].

use super::*;

/// A counter for a file of `size`, and everything it reports.
fn counter(size: Option<u64>) -> (FileCounter, Arc<Mutex<Vec<FileProgress>>>) {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&seen);
    let callback: ProgressCallback = Arc::new(move |p| sink.lock().unwrap().push(p));
    (FileCounter::new(size, Some(callback)), seen)
}

fn last(seen: &Arc<Mutex<Vec<FileProgress>>>) -> FileProgress {
    *seen.lock().unwrap().last().expect("a report")
}

const fn reading(written: u64, received: u64) -> RawProgress {
    RawProgress::new(written, received, None)
}

#[test]
fn a_count_never_goes_backwards() {
    let (counter, seen) = counter(Some(1_000));

    counter.observe(reading(600, 600));
    counter.observe(reading(200, 100));

    assert_eq!(
        last(&seen),
        FileProgress {
            bytes: 600,
            wire: 600,
            size: Some(1_000)
        }
    );
}

#[test]
fn a_count_stops_short_of_the_size_until_success() {
    let (counter, seen) = counter(Some(1_000));

    counter.observe(reading(1_000, 1_000));

    assert_eq!(last(&seen).bytes, 999);
}

#[test]
fn success_lands_on_the_size_on_disk() {
    let (counter, seen) = counter(None);
    counter.observe(reading(400, 400));

    counter.finish(1_024);

    assert_eq!(
        last(&seen),
        FileProgress {
            bytes: 1_024,
            wire: 400,
            size: Some(1_024)
        }
    );
}

/// The accelerator receives far ahead of what it has written.
#[test]
fn bytes_follow_written_not_received() {
    let (counter, seen) = counter(Some(10_000));

    counter.observe(reading(100, 5_000));

    assert_eq!(last(&seen).bytes, 100);
    assert_eq!(last(&seen).wire, 5_000);
}

#[test]
fn a_zero_size_is_unknown() {
    let (counter, seen) = counter(Some(0));

    counter.observe(RawProgress::new(500, 500, Some(0)));

    assert_eq!(last(&seen).size, None);
    assert_eq!(last(&seen).bytes, 500);
}

#[test]
fn the_transport_supplies_a_size_nobody_expected() {
    let (counter, seen) = counter(None);

    counter.observe(RawProgress::new(2_000, 2_000, Some(2_000)));

    assert_eq!(last(&seen).size, Some(2_000));
    assert_eq!(last(&seen).bytes, 1_999);
}

/// The size from metadata is the one the file is checked against.
#[test]
fn the_expected_size_outranks_the_transports() {
    let (counter, seen) = counter(Some(1_000));

    counter.observe(RawProgress::new(100, 100, Some(4_000)));

    assert_eq!(last(&seen).size, Some(1_000));
}

#[test]
fn restart_continues_wire_exactly() {
    let (counter, seen) = counter(Some(10_000));
    counter.observe(reading(800, 1_000));

    counter.restart();
    counter.observe(reading(100, 100));

    assert_eq!(last(&seen).wire, 1_100);
}

#[test]
fn restart_allows_one_rewind_then_holds() {
    let (counter, seen) = counter(Some(10_000));
    counter.observe(reading(9_000, 9_000));

    counter.restart();
    counter.observe(reading(0, 0));
    assert_eq!(
        last(&seen).bytes,
        0,
        "the new transport starts the file again"
    );

    counter.observe(reading(300, 300));
    counter.observe(reading(200, 300));
    assert_eq!(last(&seen).bytes, 300, "after which the count holds again");
}
