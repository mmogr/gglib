# GlobalDownloadStatus

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-components-GlobalDownloadStatus-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-components-GlobalDownloadStatus-complexity.json)

<!-- module-docs:start -->

Page-level download progress indicator showing the active download's progress bar, queue depth, and a dismissible completion summary banner. Groups multi-shard downloads into a single logical entry so the queue count is not inflated.

## Key Files

| File | Role |
|------|------|
| `GlobalDownloadStatus.tsx` | Active progress bar or completion summary; queue popover toggle |
| `DownloadQueuePopover.tsx` | Pending items grouped by model/shard; up/down reorder; per-item cancel |

`groupPendingItems()` collapses all shard items sharing a `group_id` into one queue entry with combined progress.

Speed and ETA are displayed exactly as the backend reports them, via
`formatRate` / `formatDuration` from `src/utils/format.ts`. Both are optional:
absent means the rate estimator has not warmed up, and renders as a placeholder
rather than `0`. This component computes no rate of its own — the download
manager's `RateEstimator` is the single source, so the CLI and the GUI always
agree.

<!-- module-docs:end -->
