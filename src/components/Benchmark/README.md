# Benchmark

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-components-Benchmark-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-components-Benchmark-complexity.json)

<!-- module-docs:start -->

Benchmark feature components. `BenchmarkPage.tsx` (in `src/pages/`) owns the
`compare`/`perf` config form, live results, and history table directly; the
`tune` mode (inference-parameter sweep for agentic tool-calling accuracy) is
broken out into its own submodule here for maintainability.

## Sub-directories

| Directory | Contents |
|-----------|----------|
| `Tune/` | Config form, live progress, and leaderboard components for tune-mode runs |

<!-- module-docs:end -->
