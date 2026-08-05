# tests

This directory no longer holds Rust test sources. Rust integration tests that
used to live here now live in
[`crates/gglib-integration-tests/`](../crates/gglib-integration-tests), a real
workspace member so `cargo test` actually compiles and runs them (see
[#640](https://github.com/mmogr/gglib/issues/640)).

What's left here:

- `fixtures/` — shared test fixtures loaded by in-crate Rust tests via a
  relative path from `CARGO_MANIFEST_DIR`. Keep this directory at the repo
  root; moving it breaks those tests.
- `ts/` — the TypeScript/Vitest test suite for the web UI, run via
  `npm run test:run` (see `.github/workflows/ci.yml`'s `test-frontend` job).
