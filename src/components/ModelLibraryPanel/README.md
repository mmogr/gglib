# ModelLibraryPanel

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-components-ModelLibraryPanel-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-components-ModelLibraryPanel-complexity.json)

<!-- module-docs:start -->

Two-tab left sidebar combining the local model library ("Your Models") with model acquisition ("Add Models"). The local tab provides search, sort, and filter controls with running-server status badges. The add tab embeds the HuggingFace browser and a local file uploader.

## Key Files

| File | Role |
|------|------|
| `ModelLibraryPanel.tsx` | Tab container; filter button with active-filters badge; search input |
| `ModelsListContent.tsx` | Filtered model list; selection highlight; running-server badge overlay |
| `AddDownloadContent.tsx` | Sub-tabs for HuggingFace browser and local file add |
| `ModelListSkeleton.tsx` | Shimmer skeleton for loading state |
| `SidebarTabs.tsx` | Reusable icon+label tab bar |

An `hasActiveFilters` badge on the filter button gives users a persistent indicator that filtering is active even when the popover is closed.

<!-- module-docs:end -->
