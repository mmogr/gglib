# HfModelPreview

<!-- module-docs:start -->

Detail preview card for a HuggingFace model showing metadata (params, architecture, license), available quantizations with per-quantization memory-fit indicators, tool support badges, download action buttons, the projector that comes with the selected quantization, and the companions an image model's download fetches.

## Key Files

| File | Role |
|------|------|
| `HfModelPreview.tsx` | Model header, tool badge, loading of the listing, the selected quantization |
| `QuantizationTable.tsx` | One row per quantization: weights size, shards, fit indicator, download button; a click or focus selects the row |
| `ProjectorNote.tsx` | Under the table: the projector the selected quantization's download fetches, its size, and what it costs |
| `CompanionNote.tsx` | Under the table, for a repository the daemon reads as an image model: the family, each companion's role, file name, repository and size, "already here" for one in the models directory, and the bytes the companions add |

Each quantization is classified as `fits` / `tight` / `wont_fit` / `unknown` by `useSystemMemory`, comparing available RAM against the quantization's estimated memory requirement. The size and shard columns are the weights alone; the fit counts the projector too.

The projector shown is chosen by the daemon (`HfQuantization.projector`), by the same rule the download uses, so the note names the file that is fetched. A repository with no projector shows no note.

The companions come from the listing's `image` (`HfImagePreview`), which the daemon builds from a read of the head of one quantization's weights; a repository it does not read as an image model has none and shows no companion note. The companions are the family's, so the note does not change with the selection.

<!-- module-docs:end -->
