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

The REST surface no longer keeps a hand-written mirror. `GgufModel`,
`ModelDetail`, `AppSettings`, `InferenceConfig`, `InferenceProfile`,
`SamplingExplanation`, `ServerInfo` and the HuggingFace, MCP and capability
types are all aliases over `generated/`, so the alignment table that used to
live here is now the import list itself — and it cannot drift, because the
export gate regenerates it from Rust.

What is still written by hand falls into three groups, and each is deliberate:

| Kind | Examples | Why |
|------|----------|-----|
| View-models | `ServerViewModel`, the chat types | No Rust counterpart. Assembled in the GUI from several responses. |
| Request bodies needing sparseness | `SparseInferenceConfig`, `ServeConfig` | A form clears a control with `delete`, which does not compile against the required key ts-rs renders for an `Option<T>`. This is a client need and not a wire one: inside `InferenceConfig` an absent key and a `null` are the same value to serde, so neither spelling reaches the backend differently. Contrast `UpdateSettingsRequest`, whose fields carry `double_option` and whose generated form is therefore already optional — it needs no wrapper, only the two-field narrowing recorded below. |
| Narrowings over a generated shape | `ParamProvenance.param`, `McpServer.server_type`, `SecondarySlotState` | Rust types the field as `String` or `&'static str`. The union is the more accurate claim, so it is intersected back on — `X & { field: Union }`, never `Omit<X, 'field'> & …`, which flattens a discriminated union. Each is a Rust-side `#[ts(type = …)]` away from being generated too. |
| | `UpdateSettingsRequest` | A different narrowing: two fields typed `InferenceConfig`/`InferenceProfile[]` on the generated body take their sparse form instead, because a settings save carries the parameters the form touched rather than a complete config. The other 21 fields are the generated ones untouched. |

Anything else hand-written under `types/` mirroring a wire shape is drift, not
design. The events, SSE and proxy-dashboard surfaces are the remaining ones and
have not been migrated yet.

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
