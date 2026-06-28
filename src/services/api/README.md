# api

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-services-api-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-services-api-complexity.json)

<!-- module-docs:start -->

Centralized route constants that mirror the Rust backend's HTTP contract definitions in `gglib-core::contracts::http`. Provides a single source of truth for all API endpoint paths consumed by the transport layer, eliminating hardcoded URL strings and enabling IDE refactoring support.

## Key Files

| File | Role |
|------|------|
| `routes.ts` | Exports path constants (e.g., `/api/models/hf/search`) consumed by `transport/api/` implementations |

## Usage

```ts
import { HF_SEARCH_PATH } from '../api/routes'
// used in transport/api/models/hf.ts
const response = await client.post(HF_SEARCH_PATH, body)
```

Route constants flow one-way: defined here, consumed by `transport/api/*`, never imported by components directly.

<!-- module-docs:end -->
