# inspector

<!-- module-docs:start -->

The inspector: one subject at a time beside a place, closable, read from
the address (`app/paths` `inspectorSubject`): a model on this machine
(`/models/:id`, `local/`), its log (`/models/:id/log`, `log/`), the paired
machine's model (`/far/:id`, `far/`) or a Hugging Face repo
(`/get/:owner/:name`, `hf/`). Its left edge drags its width between 320
and 720 px, remembered per viewer; the log opens at 60% of the window,
bounded by the window alone (at least 320 px, 400 px left for the rest),
and leaving it returns to the remembered width.

Every fact and word it shows is the daemon's, arranged here: the launch
from `GET /api/models/:id/explain`, a loaded model's launch from the server
list, state words from `components/status/words`.

Words of the Load with options sheet with no Rust twin: its title and
field labels ("Context", "Jinja", "Speculative", "MTP", "Draft tokens",
"Min probability", "Port", "Auto (from N)"), "Pin the endpoint to this
model" and its line, "This load only" and its line, the sentences that stop
a load, "Didn't save:", "Didn't load:", "Didn't install:", "Change for this
model…", and the image runtime's two sentences. The inspector's own: "▲
<parts> missing." under an image model's Load, "Show log" on a failed
load's line, "Log" before the server's last line, "Inspector width". Each
subject folder's README names its own.

| File | Role |
|------|------|
| `Inspector.tsx` | The subject switch, read from the address, a fresh inspector per subject (one line when the library does not hold the model), in an `<aside>` whose left edge drags its width |
| `useInspectorWidth.ts` | The width: dragged or arrowed between 320 and 720 px and remembered (`localStorage`, in try/catch, a stored width held to the bounds); 60% of the window for the log, bounded by the window, not remembered |
| `LoadOptionsSheet.tsx` | Load with options, in the inspector's place: Context, Jinja, Speculative, the sampling form, ▸ Port, Pin the endpoint, This load only (unchecked saves the typed values as the model's own first); each field showing what it would otherwise use and whose choice that is; Install llama.cpp in its Load's place when llama.cpp is missing, then the load; a failed save or load stays in the sheet |
| `loadOptions.ts` | Pure: what was typed, the launch request of only those values, the per-model patch of only those fields, and what stops a load |
| `Speculative.tsx` | The sheet's MTP on or off, draft tokens and min probability over the plan's `mtp` arguments, and Reset |
| `ImageParts.tsx` | The sheet's Parts line for an image model: family, files (a missing one named), and the image runtime, installed in place when missing |

| Directory | Contents |
|-----------|----------|
| `local/` | A model on this machine: header, fit sentence, Load and Chat, How it runs with its launch line, Change in place, ▸ Log, Details, the ⋯ menu |
| `log/` | A model's llama-server output: Following and Paused, copy, Clear view, the time before each line |
| `far/` | The paired machine's model: its reach, the actions it lists, Load and its re-read, Details |
| `hf/` | A Hugging Face repo: the summary, Pick a size with the daemon's fit and gglib's pick (★, selected first), Download on the selected row, projector and companions |

<!-- module-docs:end -->
