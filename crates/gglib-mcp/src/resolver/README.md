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
- `platform`: The rules a program is found by on each OS, as a value
- `search`: Where a command is looked for, and in what order
- `resolve`: Main resolution logic and orchestration

The rules are a `Platform`: the extensions a command is also tried with
(Windows: `PATHEXT`, or `.COM;.EXE;.BAT;.CMD`, each tried before the bare
name), how a file is known to be a program (Windows: by one of those
extensions; Unix: by an execute bit), the `PATH` separator, the default
directories and whether `/etc/paths` is read (macOS). `Platform::host()` is
the one place that asks which OS this is; everything else takes the rules as
a value, so Windows's are tested on every OS with `Platform::windows()`.
`path.rs` takes the executable check and the separator from here, and on
macOS the directory list; `McpService` takes the separator.

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
