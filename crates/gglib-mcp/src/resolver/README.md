# resolver

<!-- module-docs:start -->

Executable path resolution for MCP server commands.

Provides a robust, testable way to resolve command names (like `npx`) to absolute executable paths across different platforms and installation methods.

## Architecture

```text
┌─────────────────────────────────────────────────────────────────────────────────────┐
│                              resolver/                                              │
├─────────────────────────────────────────────────────────────────────────────────────┤
│                                                                                     │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐                 │
│  │   types     │  │     env     │  │     fs      │  │   search    │                 │
│  │ResolveResult│  │ EnvProvider │  │ FsProvider  │  │  Platform-  │                 │
│  │  Attempt    │  │ (injectable)│  │ (injectable)│  │  specific   │                 │
│  └─────────────┘  └─────────────┘  └─────────────┘  └─────────────┘                 │
│                                                                                     │
└─────────────────────────────────────────────────────────────────────────────────────┘
```

## Key Function

`resolve_executable("npx", &[])` → Searches PATH, nvm, homebrew, etc.

## Testability

`EnvProvider` and `FsProvider` traits allow injecting mock environment and filesystem for deterministic testing.

<!-- module-docs:end -->

<details>
<summary><h2>Modules</h2></summary>

<!-- module-table:start -->
| Module | LOC | Complexity | Coverage |
|--------|-----|------------|----------|
| [`env.rs`](env) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-mcp-resolver-env-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-mcp-resolver-env-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-mcp-resolver-env-coverage.json) |
| [`fs.rs`](fs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-mcp-resolver-fs-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-mcp-resolver-fs-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-mcp-resolver-fs-coverage.json) |
| [`resolve.rs`](resolve) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-mcp-resolver-resolve-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-mcp-resolver-resolve-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-mcp-resolver-resolve-coverage.json) |
| [`search.rs`](search) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-mcp-resolver-search-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-mcp-resolver-search-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-mcp-resolver-search-coverage.json) |
| [`types.rs`](types) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-mcp-resolver-types-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-mcp-resolver-types-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-mcp-resolver-types-coverage.json) |
<!-- module-table:end -->

</details>
