//! A row's numbers and words, from its facts.

use super::*;

const GIB: u64 = 1024 * 1024 * 1024;

fn id() -> DownloadId {
    DownloadId::new("owner/zeta-GGUF", Some("Q8_0"))
}

fn downloading(id: &DownloadId, bytes: u64, total: Option<u64>) -> RowFacts<'_> {
    RowFacts {
        id,
        phase: DownloadPhase::Downloading,
        position: 1,
        place: None,
        bytes,
        total,
        speed_bps: Some(118_400_000.0),
        eta_seconds: Some(160.0),
        notice: None,
    }
}

#[test]
fn a_downloading_row_reads_its_bytes_speed_and_time() {
    let id = id();
    let facts = RowFacts {
        place: Some(FilePlace::Part { number: 2, of: 3 }),
        ..downloading(&id, 7 * GIB, Some(28 * GIB))
    };

    let row = row(&facts);

    assert_eq!(row.id, "owner/zeta-GGUF:Q8_0");
    assert_eq!(row.model_id, "owner/zeta-GGUF");
    assert_eq!(row.quantization.as_deref(), Some("Q8_0"));
    assert_eq!(
        (row.downloaded_bytes, row.total_bytes),
        (7 * GIB, Some(28 * GIB))
    );
    assert_eq!(row.percent, Some(25.0));
    assert_eq!(row.speed_bps, Some(118_400_000.0));
    assert_eq!(
        row.text,
        DownloadRowText {
            title: "owner/zeta-GGUF:Q8_0".to_string(),
            file: Some("part 2/3".to_string()),
            status: "Downloading".to_string(),
            bytes: "7.00 GiB / 28.00 GiB".to_string(),
            percent: "25.0%".to_string(),
            speed: "118.4 MB/s".to_string(),
            eta: "ETA 2m 40s".to_string(),
        }
    );
}

/// Once the bytes are in, the phase is the status, and the last speed and
/// time remaining are dropped: nothing is transferring.
#[test]
fn a_finalizing_download_says_so_and_has_no_rate() {
    let id = id();
    for (phase, status) in [
        (DownloadPhase::Finalizing, "Finalizing…"),
        (DownloadPhase::Registering, "Registering…"),
    ] {
        let facts = RowFacts {
            phase,
            ..downloading(&id, 28 * GIB, Some(28 * GIB))
        };

        let row = row(&facts);

        assert_eq!(row.phase, phase);
        assert_eq!(row.text.status, status);
        assert_eq!((row.speed_bps, row.eta_seconds), (None, None));
        assert_eq!((row.text.speed.as_str(), row.text.eta.as_str()), ("", ""));
        assert_eq!(row.text.percent, "100.0%");
    }
}

/// With no size there is no total, no percentage and no bar to fill: the
/// bytes are shown alone. A total of 0 is the same.
#[test]
fn an_unknown_size_gives_no_total() {
    let id = id();
    for total in [None, Some(0)] {
        let row = row(&downloading(&id, 3 * GIB, total));

        assert_eq!(row.total_bytes, None);
        assert_eq!(row.percent, None);
        assert_eq!(row.text.bytes, "3.00 GiB");
        assert_eq!(row.text.percent, "");
    }
}

/// One byte short of 28 GiB is 99.9%, where rounding would say 100.0%.
#[test]
fn percent_text_stays_below_100_until_finish() {
    let id = id();
    let total = 28 * GIB;

    let short = row(&downloading(&id, total - 1, Some(total)));
    let done = row(&downloading(&id, total, Some(total)));

    assert_eq!(short.text.percent, "99.9%");
    assert_eq!(done.text.percent, "100.0%");
    assert_eq!(
        row(&downloading(&id, total / 8, Some(total))).text.percent,
        "12.5%"
    );
    assert_eq!(row(&downloading(&id, 0, Some(total))).text.percent, "0.0%");
}

#[test]
fn a_file_place_is_named() {
    let id = id();
    let named = |place| {
        let facts = RowFacts {
            place: Some(place),
            ..downloading(&id, 0, None)
        };
        DownloadRowText::of(&facts).file
    };

    assert_eq!(
        named(FilePlace::Part { number: 1, of: 5 }).as_deref(),
        Some("part 1/5")
    );
    assert_eq!(named(FilePlace::Weights).as_deref(), Some("weights"));
    assert_eq!(named(FilePlace::Projector).as_deref(), Some("projector"));
    assert_eq!(named(FilePlace::Parts(3)).as_deref(), Some("3 parts"));
    assert_eq!(DownloadRowText::of(&downloading(&id, 0, None)).file, None);
}

/// A waiting row has moved nothing: its size is shown, and no percentage,
/// speed or time.
#[test]
fn a_waiting_row_reads_its_size() {
    let id = id();

    let row = row(&RowFacts::waiting(
        &id,
        3,
        Some(FilePlace::Parts(2)),
        Some(5 * GIB),
    ));

    assert_eq!((row.phase, row.position), (DownloadPhase::Queued, 3));
    assert_eq!(row.text.status, "Queued");
    assert_eq!(row.text.bytes, "5.00 GiB");
    assert_eq!(row.text.file.as_deref(), Some("2 parts"));
    let rest = [&row.text.percent, &row.text.speed, &row.text.eta];
    assert!(rest.iter().all(|text| text.is_empty()), "{rest:?}");
}

/// A note stands in for the status while it lasts, and an unmeasured speed
/// is a dash, not a zero.
#[test]
fn a_notice_is_the_status_and_an_unknown_rate_is_a_dash() {
    let id = id();
    let facts = RowFacts {
        notice: Some("preparing fast downloader…"),
        speed_bps: None,
        eta_seconds: None,
        ..downloading(&id, 0, Some(GIB))
    };

    let text = DownloadRowText::of(&facts);

    assert_eq!(text.status, "preparing fast downloader…");
    assert_eq!(text.speed, "—");
    assert_eq!(text.eta, "ETA —");
}

#[test]
fn a_download_without_a_quantization_is_titled_by_its_repository() {
    let id = DownloadId::from_model("owner/zeta-GGUF");

    assert_eq!(download_title(&id), "owner/zeta-GGUF");
    assert_eq!(
        row(&RowFacts::waiting(&id, 1, None, None)).quantization,
        None
    );
}
