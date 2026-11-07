# hf

<!-- module-docs:start -->

The inspector's Hugging Face subject: a repo, at `/get/:owner/:name`, so
back, forward and reload keep it. From the top: `owner/name` ↗ and ×; the
summary (the Hub's parameter count, and "calls tools" when the daemon read
the chat template so, its confidence on hover); "Pick a size"; the
projector the selected variant brings; an image model's companions; ▸ About
this model (the Hub's description, counts and tags), closed.

"Pick a size" is the daemon's listing (`GET /api/models/hf/quantizations`),
one row per variant: its label, its size (its files on hover) and the
daemon's fit verdict in its own words (`fit_word`), always shown. The size
and the fit are the weights alone, judged as `gglib model download` picks a
variant. gglib's pick carries ★: the variant `gglib model download <repo>`
takes without `-q`, which the daemon marks (`picked`, from `pick_variant`
over the heads the listing read; the CLI's `--list-quants` marks it too),
marked only when those heads can tell it. Unmarked, the largest variant
that fits is selected, quantized before full precision (F16, BF16, F32),
as the download's picker prefers: a GUI-only rule until the listing can
name the pick for every repository. The selected row is scrolled into view; only the selected row
carries `[■ Download]`, which queues that variant. Sizes are the library's
(`formatSize`, two decimals). A draft or helper (`role: Sidecar`, its header's word) is dimmed,
"not a model on its own", and cannot be selected; a projector is no row.
While the daemon says the queue is full, Download is disabled and one line
says why. A repo with no model variant says "This repo has no GGUF files."

The summary comes with the pick that opened the repo (the navigation's
state), or is read by the address on a reload; the listing and the tool
reading are read once per repo.

Words with no Rust twin: "Pick a size", "Quant", "Size", "Fit", "gglib's pick" (the CLI prints the same), "not a model
on its own", "Brings <file> (<size>) for images", "The download queue is
full; Download waits for room.", "This repo has no GGUF files.", "Reading
the repo's files…", "About this model", "<N>% confident", and the companion
note's sentences.

| File | Role |
|------|------|
| `HfInspector.tsx` | The subject: header, summary, the listing's reads, Download (queue and toast), ▸ About this model |
| `PickASize.tsx` | The table: one row per variant, the daemon's fit as a shape and its words, drafts dimmed, the first pick, Download on the selected row, the queue-full line |
| `ProjectorNote.tsx` | One line under the table: the projector the selected variant brings and its size, its cost on hover |
| `CompanionNote.tsx` | For a repo the daemon reads as an image model: the family, each companion's role, file, repository and size, "already here" for one in the models directory, and the bytes the companions add |

<!-- module-docs:end -->
