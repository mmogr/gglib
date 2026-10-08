# HfModelPreview

<!-- module-docs:start -->

Detail preview card for a HuggingFace model showing metadata (params, architecture, license), available quantizations with per-quantization memory-fit indicators, tool support badges, download action buttons, and the projector that comes with the selected quantization.

## Key Files

| File | Role |
|------|------|
| `HfModelPreview.tsx` | Model header, tool badge, loading of the listing, the selected quantization |
| `QuantizationTable.tsx` | One row per quantization: weights size, shards, fit indicator, download button; a click or focus selects the row |
| `ProjectorNote.tsx` | Under the table: the projector the selected quantization's download fetches, its size, and what it costs |

Each quantization is classified as `fits` / `tight` / `wont_fit` / `unknown` by `useSystemMemory`, comparing available RAM against the quantization's estimated memory requirement. The size and shard columns are the weights alone; the fit counts the projector too.

The projector shown is chosen by the daemon (`HfQuantization.projector`), by the same rule the download uses, so the note names the file that is fetched. A repository with no projector shows no note.

<!-- module-docs:end -->
