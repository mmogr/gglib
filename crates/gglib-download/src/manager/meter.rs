//! The meter of one download: its bytes, speed and time remaining over
//! every file of it.
//!
//! A download is fetched one file at a time, and each file's counts start
//! from nothing. The meter carries what the earlier files came to, so the
//! download's bytes do not fall back at a file boundary and its speed is not
//! measured afresh for each file.

use std::time::Instant;

use gglib_core::download::ShardInfo;

use crate::executor::FileProgress;
use crate::meter::Meter;
use crate::queue::Reading;

/// One download's meter, from its first file to its last.
#[derive(Debug, Clone)]
pub(crate) struct GroupMeter {
    /// The speed and the time remaining, each fed its own count.
    meter: Meter,
    /// Bytes on disk of the files already in.
    done_bytes: u64,
    /// Bytes received for the files already in.
    done_wire: u64,
    /// The latest reading of the file in flight.
    file: FileProgress,
    /// The size of every file together, when every one is known.
    total: Option<u64>,
    /// The download is one file, so that file's size is the download's.
    lone_file: bool,
    /// A note standing in for progress on the file in flight.
    notice: Option<String>,
}

impl GroupMeter {
    /// A meter for a download of `total` bytes, with its baseline at `now`.
    ///
    /// `lone_file` says the download is one file. Its size is then taken
    /// from the transfer when the metadata had none.
    pub(crate) const fn new(total: Option<u64>, lone_file: bool, now: Instant) -> Self {
        Self {
            meter: Meter::new(now),
            done_bytes: 0,
            done_wire: 0,
            file: FileProgress {
                bytes: 0,
                wire: 0,
                size: None,
            },
            total,
            lone_file,
            notice: None,
        }
    }

    /// A meter for the download whose first file is at `place` in its group,
    /// which carries the group's size and whether the file is alone in it.
    /// A file with no place is a download of that one file.
    pub(crate) fn for_group(place: Option<&ShardInfo>, now: Instant) -> Self {
        Self::new(
            place.and_then(|place| place.group_total_bytes),
            place.is_none_or(ShardInfo::is_alone),
            now,
        )
    }

    /// Take the latest reading of the file in flight, on every tick, moved
    /// or not.
    ///
    /// The speed is fed the bytes received and the time remaining the bytes
    /// on disk, both counted over the whole download.
    pub(crate) fn observe(&mut self, file: FileProgress, notice: Option<&str>, now: Instant) {
        self.file = file;
        self.notice = notice.map(str::to_string);
        self.meter.record(
            self.done_wire + file.wire,
            self.bytes(),
            self.total().unwrap_or(0),
            now,
        );
    }

    /// The file in flight is in place: its counts join those of the files
    /// before it, and the next file starts from nothing.
    pub(crate) fn file_done(&mut self) {
        self.done_bytes += self.file.bytes;
        self.done_wire += self.file.wire;
        self.file = FileProgress::default();
        self.notice = None;
    }

    /// What the meter reads now.
    pub(crate) fn reading(&self) -> Reading {
        Reading {
            bytes: self.bytes(),
            total: self.total(),
            speed_bps: self.meter.speed_bps(),
            eta_seconds: self.meter.eta_seconds(),
            notice: self.notice.clone(),
        }
    }

    const fn bytes(&self) -> u64 {
        self.done_bytes + self.file.bytes
    }

    fn total(&self) -> Option<u64> {
        self.total
            .or_else(|| self.file.size.filter(|_| self.lone_file))
    }
}

#[cfg(test)]
#[path = "meter_tests.rs"]
mod tests;
