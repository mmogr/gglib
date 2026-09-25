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

Dialect selection is data-driven: a [`crate::domain::dialect::DialectSpec`]
— detected from the model's own chat template at import time and persisted
per model — configures the delimited parser, and the same spec generates
the decode-time GBNF grammar, so parsing and enforcement cannot drift.

## Module map

- [`tags`] — `format:*` constants, the legacy-row fallback vocabulary.
- [`error`] — non-fatal [`error::NormalizationError`] surfaced from parsers.
- [`parser`] — the [`parser::ToolCallParser`] trait + [`parser::ParserOutput`].
- [`parsers`] — concrete parser implementations, one file per dialect family.
- [`registry`] — the single dispatch site: spec → parser, plus the
  tag → builtin-spec fallback map.
- [`residue`] — chunk-safe scanner for dialect markup that survived
  normalization into client-visible text (the proxy's drift alarm).

## Adding a new dialect

**Usually: no code.** A dialect whose template renders tool calls as
`MARKERS{json}MARKERS` is derived automatically by the template probe in
`gglib-gguf` and arrives here as a spec — [`registry::get_parser`] drives
the delimited parser with it.

For a new **builtin** (a dialect that needs a fallback tag because its
templates are often stripped):

1. Add a `pub const FORMAT_*` to [`tags`].
2. Map the tag to a spec in [`registry::dialect_for_tags`].

Only a genuinely new **body codec** (a non-JSON, non-inner-XML body
encoding) needs parser code — a `BodyCodec` variant plus its decoder in
[`parsers`]; see `CONTRIBUTING.md`'s architecture-registry section.

The registry is the only place that knows the full set of parsers, by
design — see the module docs there.

Future work: `<think>` handling still lives outside the spec —
[`stream`] strips think tags unconditionally and `history` keeps its own
marker constants. Folding reasoning markers into `DialectSpec` is
deliberate follow-up scope, since it changes behaviour for untagged
models.

<!-- module-docs:end -->
