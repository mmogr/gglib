# hooks

<!-- module-docs:start -->

State management hooks for the HuggingFace model browser.

## Key Files

| File | Role |
|------|------|
| `useHuggingFaceSearch.ts` | Search query, sort/filter state, the kind of model searched for (chat or image; a change searches again), API calls, pagination cursor, direct-download intent detection |

All state mutations go through this hook — the browser component is purely presentational.

<!-- module-docs:end -->
