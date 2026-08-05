//! Progress monitor for downloads running on the gglib daemon.
//!
//! `gglib model download` queues on the daemon and watches the queue by
//! polling `GET /api/models/downloads` — the same snapshot the dashboard
//! renders. The download itself belongs to the daemon: Ctrl-C here (or a
//! closed terminal) detaches the monitor and the download keeps going,
//! which is the point of daemon ownership.

use std::collections::HashMap;
use std::time::Duration;

use anyhow::{Context, Result};
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};

use gglib_core::download::{DownloadStatus, QueueSnapshot};

use crate::daemon_client::DaemonHandle;

/// Poll interval for queue snapshots. Half a second keeps the bars lively
/// without hammering the loopback API.
const POLL_INTERVAL: Duration = Duration::from_millis(500);

/// Queue a download on the daemon.
pub async fn queue(handle: &DaemonHandle, model_id: &str, quant: Option<String>) -> Result<()> {
    let url = format!(
        "{}/api/models/downloads/queue",
        crate::daemon_client::base_url()
    );
    let response = handle
        .client
        .post(&url)
        .json(&serde_json::json!({ "model_id": model_id, "quant": quant }))
        .timeout(Duration::from_secs(30))
        .send()
        .await
        .context("queueing download on the daemon")?;
    anyhow::ensure!(
        response.status().is_success(),
        "daemon refused the download: {} {}",
        response.status(),
        response.text().await.unwrap_or_default()
    );
    Ok(())
}

/// Watch the daemon's download queue until it drains, drawing progress bars.
///
/// Exits successfully once at least one item has been observed and the queue
/// is empty again; a failure recorded for anything observed is reported as
/// an error. Ctrl-C detaches — the daemon keeps downloading.
pub async fn monitor(handle: &DaemonHandle) -> Result<()> {
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
    let url = format!("{}/api/models/downloads", crate::daemon_client::base_url());
    let multi = MultiProgress::new();
    let style = ProgressStyle::with_template(
        "  {msg:32!} [{bar:30}] {bytes}/{total_bytes} ({bytes_per_sec})",
    )
    .unwrap_or_else(|_| ProgressStyle::default_bar())
    .progress_chars("=> ");

    let mut bars: HashMap<String, ProgressBar> = HashMap::new();
    let mut seen_items = false;
    let mut observed: Vec<String> = Vec::new();

    loop {
        let snapshot: QueueSnapshot = handle
            .client
            .get(&url)
            .timeout(Duration::from_secs(5))
            .send()
            .await
            .context("polling the daemon download queue")?
            .json()
            .await
            .context("decoding the daemon download queue")?;

        for item in &snapshot.items {
            seen_items = true;
            if !observed.contains(&item.id) {
                observed.push(item.id.clone());
            }
            let bar = bars.entry(item.id.clone()).or_insert_with(|| {
                let bar = multi.add(ProgressBar::new(item.total_bytes.max(1)));
                bar.set_style(style.clone());
                bar.set_message(item.display_name.clone());
                bar
            });
            if item.total_bytes > 0 {
                bar.set_length(item.total_bytes);
            }
            bar.set_position(item.downloaded_bytes);
            if matches!(item.status, DownloadStatus::Queued) {
                bar.set_message(format!("{} (queued)", item.display_name));
            } else {
                bar.set_message(item.display_name.clone());
            }
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
