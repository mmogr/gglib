# components

<!-- module-docs:start -->

The old inspector's parts still in use, each until the PR that replaces it.

## Key Files

| File | Role | Removed by |
|------|------|------------|
| `ModelMetadataGrid.tsx` | Read-only grid of a far model's detail: size, architecture, quantization with its bits per weight, context window, HF link, revision and times | I4 |
| `InfoRow.tsx` | One label/value row, the unit the metadata grid is built from | I4 |
| `MetadataSection.tsx` | Groups `InfoRow`s under a heading | I4 |

<!-- module-docs:end -->
