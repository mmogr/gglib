# ModelInspectorPanel

<!-- module-docs:start -->

What is left of the inspector before its redesign, each file kept until the
PR that replaces it. A model of this machine is `src/inspector/` now, its
Load with options sheet included; what stays here is the paired machine's
model and the Hugging Face preview (until the inspector holds those
subjects itself, I4).

| File | Role | Removed by |
|------|------|------------|
| `FarModelInspector.tsx` | The paired machine's model, read-only: its detail from that machine through `ModelMetadataGrid` (no path, no projector or components row, no sampling section), the Vision chip when that machine's listing says it reads images (as its library row does), "Serving on" that machine, Chat and Load only where that machine's actions list them and disabled while its rows are away or stale, the `gglib chat <id> --remote` that does the same from a terminal, and a re-read after Load | I4 |
| `HfInspector.tsx` | The Hugging Face preview in the inspector's place, with the download queue's room for its Download buttons | I4 |
| `index.ts` | Exports `FarModelInspector` and `HfInspector` for the library place | I4 |

| Directory | Contents |
|-----------|----------|
| `components/` | The metadata grid `FarModelInspector` draws with (I4) |
| `hooks/` | The Hugging Face preview's download (I4) |

<!-- module-docs:end -->
