# HuggingFaceBrowser

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-components-HuggingFaceBrowser-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-components-HuggingFaceBrowser-complexity.json)

<!-- module-docs:start -->

Full-page model browser for searching and browsing GGUF models on HuggingFace Hub. Supports free-text search, parameter count filtering, sort options (downloads, likes, modified, created, alphabetical), and load-more pagination. Also handles `user/repo:quant` shorthand for direct download without browsing.

## Key Files

| File | Role |
|------|------|
| `HuggingFaceBrowser.tsx` | Search UI, filter controls, model card grid, load-more pagination |
| `ModelCardSkeleton.tsx` | Shimmer placeholder during search API calls |

## Sub-directories

| Directory | Contents |
|-----------|----------|
| `components/` | `ModelCard` — clickable card with name, params, tool support, download/like counts |
| `hooks/` | `useHuggingFaceSearch` — search/filter state, API calls, pagination, direct-download intent |

Typing `owner/repo:Q4_K_M` is detected as a direct download intent and skips browsing, opening the download flow for that specific quantization.

<!-- module-docs:end -->
