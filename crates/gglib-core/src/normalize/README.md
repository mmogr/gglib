# normalize

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-normalize-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-normalize-complexity.json)

<!-- module-docs:start -->

Universal local-LLM consistency layer.

This module rewrites model-specific output dialects into the strict
OpenAI-shaped [`crate::domain::agent::LlmStreamEvent`] sequence that the
rest of the codebase expects.  Adapters wrap the LLM stream once at the
port boundary; every downstream surface (Axum, CLI, Tauri, proxy)
consumes the canonical form.

## Module map

- [`tags`] — `format:*` constants used to pick a parser.
- [`error`] — non-fatal [`error::NormalizationError`] surfaced from parsers.
- [`parser`] — the [`parser::ToolCallParser`] trait + [`parser::ParserOutput`].
- [`parsers`] — concrete parser implementations, one file per dialect.
- [`registry`] — the single dispatch site that maps tags to parsers.

## Adding a new dialect

1. Add a `pub const FORMAT_*` to [`tags`].
2. Drop a new file under [`parsers`].
3. Add **one** match arm to [`registry::get_parser`].

The registry is the only place that knows the full set of parsers, by
design — see the module docs there.

<!-- module-docs:end -->

<details>
<summary><h2>Modules</h2></summary>

<!-- module-table:start -->
| Module | LOC | Complexity | Coverage |
|--------|-----|------------|----------|
| [`error.rs`](error.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-normalize-error-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-normalize-error-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-normalize-error-coverage.json) |
| [`history.rs`](history.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-normalize-history-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-normalize-history-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-normalize-history-coverage.json) |
| [`parser.rs`](parser.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-normalize-parser-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-normalize-parser-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-normalize-parser-coverage.json) |
| [`registry.rs`](registry.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-normalize-registry-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-normalize-registry-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-normalize-registry-coverage.json) |
| [`stream.rs`](stream.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-normalize-stream-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-normalize-stream-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-normalize-stream-coverage.json) |
| [`tags.rs`](tags.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-normalize-tags-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-normalize-tags-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-normalize-tags-coverage.json) |
| [`parsers/`](parsers/) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-parsers-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-parsers-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-parsers-coverage.json) |
<!-- module-table:end -->

</details>
