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

This module is crate-internal, so the example below is illustrative rather than
executable — a doctest compiles as its own crate and cannot name a `pub(crate)`
module. Callers outside `gglib-mcp` reach resolution through `McpService`.

```text
use crate::resolver::resolve_executable;

// Resolve "npx" to absolute path
let result = resolve_executable("npx", &[]).unwrap();
println!("Resolved to: {}", result.resolved_path.display());

// Show diagnostic info
for attempt in &result.attempts {
    println!("  {} - {}", attempt.candidate.display(), attempt.outcome);
}
```

<!-- module-docs:end -->
