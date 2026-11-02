# testdata

<!-- module-docs:start -->

Fixtures the domain's tests read with `include_str!`.

- `image_families/` — the tensor tables of eleven image-model files (eight
  measured, three cut from their headers alone), one sorted
  `name<TAB>shape` list per file, which `image_family_goldens.rs` reads for
  the tests that sniff a family or check a component's role.

<!-- module-docs:end -->
