# inspector

<!-- module-docs:start -->

The inspector: one subject at a time beside a place, closable. Today that
subject is a model on this machine (`local/`); the paired machine's model, a
Hugging Face repo and a loaded model's log join it in I4, and are wired in
`places/library/LibraryPlace` (from `components/ModelInspectorPanel/`) until
then.

Every fact and word it shows is the daemon's, arranged here: the launch
from `GET /api/models/:id/explain`, a loaded model's launch from the server
list, state words from `components/status/words`.

Words of the Load with options sheet with no Rust twin: its title and
field labels ("Context", "Jinja", "Speculative", "MTP", "Draft tokens",
"Min probability", "Port", "Auto (from N)"), "Pin the endpoint to this
model" and its line, "This load only" and its line, the sentences that stop
a load, "Didn't save:", "Didn't load:", "Didn't install:", "Change for this
model…", and the image runtime's two sentences. The inspector's own: "▲
<parts> missing." under an image model's Load.

| File | Role |
|------|------|
| `Inspector.tsx` | The subject switch: a model on this machine, a fresh inspector per model, or one line when the library does not hold it |
| `LoadOptionsSheet.tsx` | Load with options, in the inspector's place: Context, Jinja, Speculative, the sampling form, ▸ Port, Pin the endpoint, This load only (unchecked saves the typed values as the model's own first); each field showing what it would otherwise use and whose choice that is; Install llama.cpp in its Load's place when llama.cpp is missing, then the load; a failed save or load stays in the sheet |
| `loadOptions.ts` | Pure: what was typed, the launch request of only those values, the per-model patch of only those fields, and what stops a load |
| `Speculative.tsx` | The sheet's MTP on or off, draft tokens and min probability over the plan's `mtp` arguments, and Reset |
| `ImageParts.tsx` | The sheet's Parts line for an image model: family, files (a missing one named), and the image runtime, installed in place when missing |

| Directory | Contents |
|-----------|----------|
| `local/` | A model on this machine: header, fit sentence, Load and Chat, How it runs with its launch line, Change in place, Details, the ⋯ menu |

<!-- module-docs:end -->
