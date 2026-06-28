# resolver

<!-- module-docs:start -->

Executable path resolution for MCP server commands.

This module provides a robust, testable way to resolve command names
(like "npx") to absolute executable paths across different platforms
and installation methods.

## Architecture

The resolver is split into small, focused modules:
- `types`: Core types (`ResolveResult`, `Attempt`, `AttemptOutcome`)
- `env`: Environment variable access trait (injectable for testing)
- `fs`: Filesystem operations trait (injectable for testing)
- `search`: Platform-specific search strategies
- `resolve`: Main resolution logic and orchestration

## Usage

```rust,no_run
use gglib_mcp::resolver::resolve_executable;

// Resolve "npx" to absolute path
let result = resolve_executable("npx", &[]).unwrap();
println!("Resolved to: {}", result.resolved_path.display());

// Show diagnostic info
for attempt in &result.attempts {
    println!("  {} - {}", attempt.candidate.display(), attempt.outcome);
}
```

<!-- module-docs:end -->

<details>
<summary><h2>Modules</h2></summary>

<!-- module-table:start -->
| Module | LOC | Complexity | Coverage |
|--------|-----|------------|----------|
| [`env.rs`](env.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-mcp-resolver-env-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-mcp-resolver-env-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-mcp-resolver-env-coverage.json) |
| [`fs.rs`](fs.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-mcp-resolver-fs-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-mcp-resolver-fs-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-mcp-resolver-fs-coverage.json) |
| [`resolve.rs`](resolve.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-mcp-resolver-resolve-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-mcp-resolver-resolve-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-mcp-resolver-resolve-coverage.json) |
| [`search.rs`](search.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-mcp-resolver-search-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-mcp-resolver-search-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-mcp-resolver-search-coverage.json) |
| [`types.rs`](types.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-mcp-resolver-types-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-mcp-resolver-types-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-mcp-resolver-types-coverage.json) |
<!-- module-table:end -->

</details>
