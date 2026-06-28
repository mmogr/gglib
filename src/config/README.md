# config

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-config-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-config-complexity.json)

<!-- module-docs:start -->

Environment-aware backend URL configuration. Abstracts deployment-environment differences (production same-origin, local development, Tauri desktop) so consumers never hardcode `localhost:9887`.

## Key Files

| File | Role |
|------|------|
| `api.ts` | `getBackendPort()` — reads `VITE_GGLIB_WEB_PORT`, defaults to `9887`; `getApiBaseUrl()` — returns `''` in production or `http://localhost:PORT` in development |

| Environment | `getApiBaseUrl()` | Rationale |
|-------------|-------------------|-----------|
| Production | `''` (relative) | Works at any deployment origin |
| Development | `http://localhost:9887` | Vite dev server proxies to the running backend |
| Tauri | Dynamic discovery | Handled by the transport layer, not configured here |

<!-- module-docs:end -->
