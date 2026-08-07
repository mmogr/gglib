# parsers

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-normalize-parsers-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-normalize-parsers-complexity.json)

<!-- module-docs:start -->

Submodule index for concrete [`super::parser::ToolCallParser`] implementations.

Each parser lives in its own file and is named after the dialect *family*
it handles: [`delimited`] covers every marker-delimited dialect, configured
by a [`crate::domain::dialect::DialectSpec`], while [`standard`] is the
identity passthrough.  This is the only place — together with
[`super::registry`] — where the set of available parsers is enumerated.

<!-- module-docs:end -->

<details>
<summary><h2>Modules</h2></summary>

<!-- module-table:start -->
| Module | LOC | Complexity | Coverage |
|--------|-----|------------|----------|
| [`delimited.rs`](delimited.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-parsers-delimited-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-parsers-delimited-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-parsers-delimited-coverage.json) |
| [`standard.rs`](standard.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-parsers-standard-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-parsers-standard-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-parsers-standard-coverage.json) |
<!-- module-table:end -->

</details>
