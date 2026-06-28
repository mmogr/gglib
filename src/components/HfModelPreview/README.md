# HfModelPreview

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-components-HfModelPreview-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-components-HfModelPreview-complexity.json)

<!-- module-docs:start -->

Detail preview card for a HuggingFace model showing metadata (params, architecture, license), available quantizations with per-quantization memory-fit indicators, tool support badges, and download action buttons.

## Key Files

| File | Role |
|------|------|
| `HfModelPreview.tsx` | Quantization list, fit indicators, tool badge, download triggers |

Each quantization is classified as `fits` / `tight` / `wont_fit` / `unknown` by `useSystemMemory`, comparing available RAM against the quantization's estimated memory requirement.

<!-- module-docs:end -->
