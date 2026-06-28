# models

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-services-transport-api-models-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-services-transport-api-models-complexity.json)

<!-- module-docs:start -->

Splits model API operations into two domains — local GGUF models and HuggingFace Hub models — and composes them into the single `ModelsTransport` interface. The separation keeps filesystem and remote-API concerns independent while presenting a unified surface to callers.

## Key Files

| File | Role |
|------|------|
| `index.ts` | Composes `local` and `hf` into a single `ModelsTransport` implementation |
| `local.ts` | `GET/POST/PUT/DELETE /api/models` — list, get, add, remove, update, search, filter options |
| `hf.ts` | `POST /api/models/hf/*` — search Hub, get model summary, list quantizations, tool support |

## Domain Split

```
ModelsTransport
    ├── local.ts  ── filesystem-backed models (GGUF files the user has added)
    └── hf.ts     ── HuggingFace Hub discovery (remote, read-only browsing)
```

Local model operations mutate local state. HuggingFace operations are always read-only and initiate downloads via `DownloadsTransport`.

<!-- module-docs:end -->
