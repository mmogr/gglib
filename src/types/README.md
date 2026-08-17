<!-- module-docs:start -->

# Types Module

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-types-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-types-complexity.json)

TypeScript type definitions shared across the gglib GUI.

## Architecture

```text
┌─────────────────────────────────────────────────────────────────────────────────────┐
│                                  types/index.ts                                     │
├─────────────────────────────────────────────────────────────────────────────────────┤
│                                                                                     │
│  ┌─────────────────────────────────────────────────────────────────────────────┐    │
│  │                             Domain Types                                    │    │
│  │  GgufModel, ServerInfo, ServeConfig, etc.                                   │    │
│  └─────────────────────────────────────────────────────────────────────────────┘    │
│                                                                                     │
│  ┌─────────────────────────────────────────────────────────────────────────────┐    │
│  │                            Settings Types                                   │    │
│  │  AppSettings, ModelsDirectoryInfo, etc.                                     │    │
│  └─────────────────────────────────────────────────────────────────────────────┘    │
│                                                                                     │
│  ┌─────────────────────────────────────────────────────────────────────────────┐    │
│  │                             Event Types                                     │    │
│  │  ServerEvent, DownloadEvent, etc.                                           │    │
│  └─────────────────────────────────────────────────────────────────────────────┘    │
│                                                                                     │
└─────────────────────────────────────────────────────────────────────────────────────┘
                                       │
                                       ▼
                          ┌───────────────────────┐
                          │    Rust Backend       │
                          │  (gglib-core types)   │
                          └───────────────────────┘
```

## Key Types

### Domain Types

| Type | Description |
|------|-------------|
| `GgufModel` | Model metadata (name, path, params, quantization, tags) |
| `ServerInfo` | A running server as `GET /api/servers` reports it — snake_case, no status |
| `ServeConfig` | Server launch configuration |

### Configuration Types

| Type | Description |
|------|-------------|
| `AppSettings` | Application preferences and defaults |
| `ModelsDirectoryInfo` | Models directory path and metadata |

### Reasoning Types (`reasoning.ts`, re-exported from `index.ts`)

| Type | Description |
|------|-------------|
| `TemplateSupport` | `'yes' \| 'no' \| 'unknown'` — whether a model's chat template reads a kwarg. Three states on purpose: capabilities are read from a *running* server, so `unknown` is the common answer and must never render as a `no` |
| `ProvenanceParamKey` | Every key the explain endpoint attributes a source to — `SamplingParamKey` plus `reasoningEffort`, which has no numeric bounds and so cannot join the bounds table |
| `SuppressedEffort` | A resolved effort level a template would ignore, with the rung that asked for it |

### Event Types

| Type | Description |
|------|-------------|
| `ServerEvent` | Server lifecycle events (running, stopped, crashed) |
| `DownloadProgress` | Download progress updates |

## Generated bindings (`generated/`)

`generated/` is written by ts-rs from the Rust wire types and committed, so the
frontend build never needs cargo. Never edit a file in it: `make bindings`
rewrites the directory, and CI runs `make bindings-check`, which regenerates
and fails on any difference.

Two things ts-rs cannot infer, and `scripts/check_ts_bindings.sh` enforces:
`i64`/`u64` become `bigint` unless a field says otherwise — a type `JSON.parse`
cannot produce — and `skip_serializing_if` does not imply optional, so a field
serde omits still emits as `field: T | null` unless it carries `#[ts(optional)]`.

The directory is exempt from the README and file-size checks and from eslint.
Generated code has no author to address a finding to, and a 400-line binding is
however long its Rust type is.

## Type Alignment

The types below are still hand-written. They are being replaced by `generated/`
imports; what remains here afterwards is view-models, which have no Rust
counterpart by design.

These TypeScript types mirror the Rust types in `gglib-core`:

```text
TypeScript                    Rust
──────────────────────────────────────────
GgufModel          ←→        Model
ServerInfo         ←→        gglib_app_services::types::ServerInfo
AppSettings        ←→        Settings
DownloadProgress   ←→        DownloadProgress
```

The `services/transport/` layer handles JSON serialization between these.

## Usage

```typescript
import type { GgufModel, ServerInfo, AppSettings } from './types';

function ModelCard({ model }: { model: GgufModel }) {
  return (
    <div>
      <h3>{model.name}</h3>
      <p>{model.param_count_b}B params</p>
    </div>
  );
}
```

## Design Principles

1. **Single Source** — All shared types exported from `index.ts`
2. **Backend Parity** — Types match Rust structs for seamless JSON exchange
3. **Strict Typing** — No `any` types; full type coverage

<!-- module-docs:end -->
