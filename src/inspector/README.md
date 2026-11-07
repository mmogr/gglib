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

| File | Role |
|------|------|
| `Inspector.tsx` | The subject switch: a model on this machine, a fresh inspector per model, or one line when the library does not hold it |

| Directory | Contents |
|-----------|----------|
| `local/` | A model on this machine: header, fit sentence, Load and Chat, How it runs with its launch line, Change in place, Details, the ⋯ menu |

<!-- module-docs:end -->
