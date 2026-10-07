# GlobalDownloadStatus

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-components-GlobalDownloadStatus-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-components-GlobalDownloadStatus-complexity.json)

<!-- module-docs:start -->

Page-level download card: the running download's bar and words, a chip for the downloads waiting behind it, and a dismissible summary of the last queue run.

## Key Files

| File | Role |
|------|------|
| `GlobalDownloadStatus.tsx` | The running download's card, or the last run's summary; queue popover toggle |
| `DownloadQueuePopover.tsx` | The waiting downloads; up/down reorder; remove from the queue |
| `index.ts` | Barrel |

The card draws the `active` row of the daemon's `QueueSnapshot`, which
`useDownloadManager` holds as it arrived. A row is a whole download: every
file of one model, with one bar. The card has no state of its own beyond
whether the popover is open.

Every string on the card about the download is the row's `text`, printed as
it arrived: `status` (`Downloading`, `Finalizing…`, `Registering…`, or a note
from the downloader such as "preparing fast downloader…"), `file` (`part 2/3`,
`weights`, `projector`), `title`, `bytes`, `percent`, `speed` and `eta`. The
CLI's download board prints the same fields of the same row, with one
difference: it leaves out the status of a plain transfer, `Downloading`, which
the card shows. Nothing here formats a byte count, a rate or a duration, picks
a label for a phase, or shortens a title; a long title is cut by CSS, with the
whole of it in the tooltip. The bar's width is the row's `percent`, as the
CLI's bar is, and the bar is indeterminate when the row has none, which is
when the download's size is not known.

Between two files of one download the row stays `active` and `downloading`,
with the speed and the time remaining it last had, so the card stays up.
While the model is finalized and registered the row is still `active`, with
an empty `speed` and `eta`.

The chip reads `+N queued`, where N is the number of rows in the snapshot's
`waiting` list, and the popover's header counts the same rows. Both counts
are the length of that list, worked out here. The popover lists those rows by `text.title`, with `text.file`
(`3 parts`) where the row has one. Reorder sends the row's `position` plus or
minus one, and remove sends the row's `id`, which takes every file of that
download out of the queue.

<!-- module-docs:end -->
