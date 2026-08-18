# tests

This directory no longer holds Rust test sources. Rust integration tests that
used to live here now live in
[`crates/gglib-integration-tests/`](../crates/gglib-integration-tests), a real
workspace member so `cargo test` actually compiles and runs them (see
[#640](https://github.com/mmogr/gglib/issues/640)).

What's left here:

- `ts/` — the TypeScript/Vitest test suite for the web UI, run via
  `npm run test:run` (see `.github/workflows/ci.yml`'s `test-frontend` job).
  Its own fixtures are in [`ts/fixtures/`](ts/fixtures).

Rust fixtures are not here and do not need to be: the one crate that has any
keeps them in `crates/gglib-proxy/tests/fixtures/`, where the
`CARGO_MANIFEST_DIR`-relative path resolves inside the crate.
