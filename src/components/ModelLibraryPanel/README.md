# ModelLibraryPanel

<!-- module-docs:start -->

Two-tab left sidebar combining the model library ("Your Models") with model acquisition ("Add Models"). The library tab lists this machine's models, with search, sort, and filter controls and running-server status badges, and after them the paired machine's, read-only. The add tab embeds the HuggingFace browser and a local file uploader.

## Key Files

| File | Role |
|------|------|
| `ModelLibraryPanel.tsx` | Tab container; filter button with active-filters badge; search input |
| `ModelsListContent.tsx` | Filtered model list; selection highlight; running-server badge overlay; a neutral Vision chip on a model that reads images, and a neutral Draws chip naming the family of one that draws them |
| `PairedModelRows.tsx` | The paired machine's models after this machine's: a group headed by its name and whether it is reached, rows badged with the machine (and with the Vision chip when the model reads images) and keyed by it and the model's id there, matched by the same search, and set aside with a note while a filter (which describes this machine's models) is on |
| `AddDownloadContent.tsx` | Sub-tabs for HuggingFace browser and local file add |
| `ModelListSkeleton.tsx` | Shimmer skeleton for loading state |
| `RecommendedModel.tsx` | The hardware-sized suggestion `gglib up` makes on a first run, surfaced where models are chosen |

An `hasActiveFilters` badge on the filter button gives users a persistent indicator that filtering is active even when the popover is closed.

<!-- module-docs:end -->
