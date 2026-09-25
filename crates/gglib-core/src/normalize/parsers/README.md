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
