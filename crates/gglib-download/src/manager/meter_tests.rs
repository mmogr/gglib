//! What a download's meter makes of its files' counts.

use std::time::{Duration, Instant};

use super::*;

const MIB: u64 = 1024 * 1024;
const TICK: Duration = Duration::from_millis(250);

const fn at(bytes: u64, wire: u64, size: u64) -> FileProgress {
    FileProgress {
        bytes,
        wire,
        size: Some(size),
    }
}

/// The second file starts from nothing, and the download's bytes carry on
/// from where the first file left them.
#[test]
fn group_bytes_continue_across_a_file_boundary() {
    let start = Instant::now();
    let mut meter = GroupMeter::new(Some(300), false, start);

    meter.observe(at(100, 100, 100), None, start);
    assert_eq!(meter.reading().bytes, 100);
    meter.file_done();
    assert_eq!(meter.reading().bytes, 100, "between the two files");

    meter.observe(at(0, 0, 200), None, start + TICK);
    assert_eq!(
        meter.reading().bytes,
        100,
        "the second file has nothing yet"
    );
    meter.observe(at(50, 50, 200), None, start + TICK * 2);

    let reading = meter.reading();
    assert_eq!((reading.bytes, reading.total), (150, Some(300)));
}

/// A file landing does not start the estimates again: between two files the
/// download still reads the speed and the time remaining it last measured.
#[test]
fn the_speed_and_time_remaining_carry_over_a_file_boundary() {
    let start = Instant::now();
    let mut meter = GroupMeter::new(Some(1000 * MIB), false, start);
    for tick in 0..=40_u32 {
        let in_so_far = u64::from(tick) * 5 * MIB;
        meter.observe(
            at(in_so_far, in_so_far, 200 * MIB),
            None,
            start + TICK * tick,
        );
    }
    let before = meter.reading();
    assert!(before.speed_bps.is_some() && before.eta_seconds.is_some());

    meter.file_done();

    let between = meter.reading();
    assert_eq!(between.bytes, 200 * MIB);
    assert_eq!(
        (between.speed_bps, between.eta_seconds),
        (before.speed_bps, before.eta_seconds)
    );
}

/// A file found whole on disk is bytes on disk and nothing received: the
/// bytes move, and there is no speed to show for it.
#[test]
fn a_cache_hit_moves_bytes_but_not_the_speed() {
    let start = Instant::now();
    let mut meter = GroupMeter::new(Some(8192 * MIB), true, start);

    for tick in 0..=12_u32 {
        let found = if tick >= 4 { 4096 * MIB } else { 0 };
        meter.observe(at(found, 0, 8192 * MIB), None, start + TICK * tick);
    }

    let reading = meter.reading();
    assert_eq!(reading.bytes, 4096 * MIB);
    assert_eq!(reading.speed_bps, None);
}

/// The accelerator as it was observed: the network steady, and the file
/// growing by 700 MiB every 13 seconds. The speed shown is the network's.
#[test]
fn speed_from_wire_eta_from_bytes() {
    // 13 seconds are 52 ticks.
    let step = 700 * MIB;
    let per_tick = u32::try_from(step / 52).expect("a tick's bytes fit in a u32");
    let network_bps = f64::from(per_tick) * 4.0;

    let start = Instant::now();
    let mut meter = GroupMeter::new(Some(100 * step), true, start);
    for tick in 0..=480_u32 {
        let elapsed = f64::from(tick) * TICK.as_secs_f64();
        let file = at(
            u64::from(tick / 52) * step,
            u64::from(per_tick) * u64::from(tick),
            100 * step,
        );

        meter.observe(file, None, start + TICK * tick);

        // Past the warm-up, the speed is the network's at every tick.
        if elapsed >= 5.0 {
            let speed = meter
                .reading()
                .speed_bps
                .expect("a speed after the warm-up");
            let off = (speed - network_bps).abs() / network_bps;
            assert!(off < 0.05, "{off:.3} off at {elapsed}s: {speed} B/s");
        }
    }

    // 9 of 100 steps are on disk after two minutes: about twenty minutes to
    // go. Counted from the bytes received it would be the same here, so the
    // two-estimator meter's own tests tell the two apart.
    let eta = meter.reading().eta_seconds.expect("the size is known");
    assert!((600.0..2400.0).contains(&eta), "{eta}s");
}

/// The speed does not start again at a file boundary: the second file's
/// first second reads the rate the first file ended at.
#[test]
fn the_speed_carries_over_a_file_boundary() {
    let per_tick = 5 * MIB;
    let size = 40 * per_tick;
    let start = Instant::now();
    let mut meter = GroupMeter::new(Some(2 * size), false, start);

    for tick in 0..=40_u32 {
        let wire = per_tick * u64::from(tick);
        meter.observe(
            at(wire.min(size - 1), wire, size),
            None,
            start + TICK * tick,
        );
    }
    meter.observe(at(size, size, size), None, start + TICK * 40);
    let before = meter
        .reading()
        .speed_bps
        .expect("a speed after ten seconds");
    meter.file_done();

    for tick in 41..=44_u32 {
        let wire = per_tick * u64::from(tick - 40);
        meter.observe(at(wire, wire, size), None, start + TICK * tick);
    }

    let after = meter.reading().speed_bps.expect("still a speed");
    assert!(
        (after - before).abs() / before < 0.1,
        "{before} then {after}"
    );
}

/// A download of one file takes its size from the transfer when the
/// metadata had none. A download of several does not: one file's size is
/// not the download's.
#[test]
fn a_lone_file_gives_the_download_its_size() {
    let start = Instant::now();
    let mut lone = GroupMeter::new(None, true, start);
    let mut several = GroupMeter::new(None, false, start);

    lone.observe(at(10, 10, 500), None, start);
    several.observe(at(10, 10, 500), None, start);

    assert_eq!(lone.reading().total, Some(500));
    assert_eq!(several.reading().total, None);
}

/// A note is the file's: it is read while it lasts, and gone with the file.
#[test]
fn a_notice_is_read_until_its_file_is_done() {
    let start = Instant::now();
    let mut meter = GroupMeter::new(Some(300), false, start);

    meter.observe(at(0, 0, 100), Some("using direct transfer…"), start);
    assert_eq!(
        meter.reading().notice.as_deref(),
        Some("using direct transfer…")
    );

    meter.observe(at(10, 10, 100), None, start + TICK);
    assert_eq!(meter.reading().notice, None);

    meter.observe(at(10, 10, 100), Some("again"), start + TICK * 2);
    meter.file_done();
    assert_eq!(meter.reading().notice, None);
}

/// A group's meter has the size of every file from its first file's place,
/// before a byte of any has moved.
#[test]
fn a_groups_meter_starts_with_the_groups_size() {
    let start = Instant::now();
    let first = ShardInfo::with_size(0, 2, "z-00001-of-00002.gguf", 100).in_group(Some(300), false);
    let mut meter = GroupMeter::for_group(Some(&first), start);
    assert_eq!(meter.reading().total, Some(300));

    // The file's own size is not the download's.
    let unsized_group = ShardInfo::new(0, 2, "z-00001-of-00002.gguf");
    meter = GroupMeter::for_group(Some(&unsized_group), start);
    meter.observe(at(10, 10, 100), None, start);
    assert_eq!(meter.reading().total, None);
}

/// A download of one file, placed or not, is as big as that file turns out
/// to be when the metadata had no size for it.
#[test]
fn a_lone_files_meter_takes_the_size_the_transfer_reports() {
    let start = Instant::now();
    let alone = ShardInfo::new(0, 1, "z.gguf");
    for place in [None, Some(&alone)] {
        let mut meter = GroupMeter::for_group(place, start);
        meter.observe(at(10, 10, 100), None, start);
        assert_eq!(meter.reading().total, Some(100));
    }
}
