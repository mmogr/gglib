# HuggingFaceBrowser

<!-- module-docs:start -->

Full-page model browser for searching and browsing GGUF models on HuggingFace Hub. Supports a Chat/Image toggle (models that chat, or models that draw, sent as the search's `kind`), free-text search, parameter count filtering, sort options (downloads, likes, modified, created, alphabetical), and load-more pagination. Also handles `user/repo:quant` shorthand for direct download without browsing.

## Key Files

| File | Role |
|------|------|
| `HuggingFaceBrowser.tsx` | Search UI, the Chat/Image toggle, filter controls, model card grid, load-more pagination |
| `ModelCardSkeleton.tsx` | Shimmer placeholder during search API calls |

## Sub-directories

| Directory | Contents |
|-----------|----------|
| `components/` | `ModelCard` — clickable card with name, params, tool support, download/like counts |
| `hooks/` | `useHuggingFaceSearch` — search/filter state and kind, API calls, pagination, direct-download intent |

Typing `owner/repo:Q4_K_M` is detected as a direct download intent and skips browsing, opening the download flow for that specific quantization.

<!-- module-docs:end -->
