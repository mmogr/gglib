//! Progress monitor for downloads running on the gglib daemon.
//!
//! Presentation only: the queue snapshot arrives from
//! [`DaemonHandle::download_queue`], and this module turns it into progress
//! bars. The download itself belongs to the daemon — Ctrl-C here (or a closed
//! terminal) detaches the monitor and the download keeps going, which is the
//! point of daemon ownership.

use std::collections::HashMap;
use std::time::Duration;

use anyhow::{Context, Result};
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};

use gglib_core::download::{DownloadStatus, QueuedDownload};
use gglib_download::{rate_suffix, total_bytes_key};

use crate::daemon_client::DaemonHandle;

/// Poll interval for queue snapshots. Matches the daemon's own progress
/// sampling tick (250ms), so the bars are at most one tick behind without
/// hammering the loopback API.
const POLL_INTERVAL: Duration = Duration::from_millis(250);

/// Watch the daemon's download queue until it drains, drawing progress bars.
///
/// Exits successfully once at least one item has been observed and the queue
/// is empty again; a failure recorded for anything observed is reported as
/// an error. Ctrl-C detaches — the daemon keeps downloading.
pub(super) async fn monitor(handle: &DaemonHandle) -> Result<()> {
    let watch = watch_queue(handle);
    tokio::select! {
        result = watch => result,
        _ = tokio::signal::ctrl_c() => {
            eprintln!();
            eprintln!("  Detached \u{2014} the download continues on the gglib daemon.");
            eprintln!("  Re-attach anytime with `gglib model download <id>` or the dashboard.");
            Ok(())
        }
    }
}

async fn watch_queue(handle: &DaemonHandle) -> Result<()> {
    let multi = MultiProgress::new();
    // No `{bytes_per_sec}`: that's indicatif's own estimate, derived from our
    // `set_position` calls, and it disagrees with the rate the daemon's
    // estimator computed for the same transfer (see the BAR_TEMPLATE comment
    // in gglib_download::cli_emitter). The daemon's rate arrives on the
    // snapshot and is rendered into the message instead.
    let style = ProgressStyle::with_template("  {msg:48!} [{bar:30}] {bytes}/{total_bytes}")
        .unwrap_or_else(|_| ProgressStyle::default_bar())
        .progress_chars("=> ")
        .with_key("total_bytes", total_bytes_key);

    let mut bars: HashMap<String, ProgressBar> = HashMap::new();
    let mut seen_items = false;
    let mut observed: Vec<String> = Vec::new();

    loop {
        let snapshot = handle
            .download_queue()
            .await
            .context("polling the daemon download queue")?;

        for item in &snapshot.items {
            seen_items = true;
            if !observed.contains(&item.id) {
                observed.push(item.id.clone());
            }
            let bar = bars.entry(item.id.clone()).or_insert_with(|| {
                // `no_length()`, never `new(0)`: indicatif renders an explicit
                // length of 0 as 100% full. Unknown totals draw an empty bar
                // and `—` (via `total_bytes_key`) until the first snapshot
                // carries a real total.
                let bar = multi.add(ProgressBar::no_length());
                bar.set_style(style.clone());
                bar
            });
            if item.total_bytes > 0 && bar.length() != Some(item.total_bytes) {
                bar.set_length(item.total_bytes);
            }
            bar.set_position(item.downloaded_bytes);
            bar.set_message(item_message(item));
        }

        // Items that left the queue are finished (or failed — checked below).
        let live: Vec<String> = snapshot.items.iter().map(|i| i.id.clone()).collect();
        bars.retain(|id, bar| {
            if live.contains(id) {
                true
            } else {
                bar.finish_and_clear();
                false
            }
        });

        // A failure recorded for something this session observed is this
        // session's failure to report.
        if let Some(failure) = snapshot
            .recent_failures
            .iter()
            .find(|f| observed.contains(&f.id))
            && seen_items
            && snapshot.items.is_empty()
        {
            anyhow::bail!(
                "download failed: {} \u{2014} {}",
                failure.display_name,
                failure.error
            );
        }

        if seen_items && snapshot.items.is_empty() {
            eprintln!("  \u{2713} download complete \u{2014} model registered by the daemon");
            return Ok(());
        }

        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

/// Label for a queue item: name, shard position, and — while downloading —
/// the rate and ETA the daemon's estimator computed.
fn item_message(item: &QueuedDownload) -> String {
    if matches!(item.status, DownloadStatus::Queued) {
        return format!("{} (queued)", item.display_name);
    }

    let shard = item.shard_info.as_ref().map_or_else(String::new, |shard| {
        format!(" [shard {}/{}]", shard.shard_index + 1, shard.total_shards)
    });

    format!(
        "{}{} {}",
        item.display_name,
        shard,
        rate_suffix(item.speed_bps, item.eta_seconds)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use gglib_core::download::ShardInfo;

    fn item(status: DownloadStatus) -> QueuedDownload {
        QueuedDownload::new("owner/repo:Q8_0", "owner/repo", "owner/repo:Q8_0", 1, 0)
            .with_status(status)
    }

    #[test]
    fn queued_items_are_labeled_queued() {
        let msg = item_message(&item(DownloadStatus::Queued));
        assert_eq!(msg, "owner/repo:Q8_0 (queued)");
    }

    #[test]
    fn downloading_items_render_placeholder_rate_during_warmup() {
        // speed/eta are None until the daemon's estimator warms up — the
        // message must show a placeholder, not 0 B/s (reads as stalled).
        let msg = item_message(&item(DownloadStatus::Downloading));
        assert!(msg.starts_with("owner/repo:Q8_0 "));
        assert!(
            !msg.contains("0 B/s"),
            "warmup must not render a zero rate: {msg}"
        );
    }

    #[test]
    fn downloading_items_render_manager_rate_and_shard() {
        let mut item = item(DownloadStatus::Downloading)
            .with_shard_info("group".into(), ShardInfo::new(0, 3, "shard-0.gguf"));
        item.update_progress(500, 1_000, Some(1_048_576.0), Some(90.0));

        let msg = item_message(&item);
        assert!(msg.contains("[shard 1/3]"), "{msg}");
        assert!(msg.contains("MB/s") || msg.contains("MiB/s"), "{msg}");
        assert!(msg.contains("ETA"), "{msg}");
    }
}
