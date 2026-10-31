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

Rust fixtures are not here and do not need to be. Two crates keep fixtures under
`tests/fixtures/`. `crates/gglib-axum/tests/fixtures/ui/` is data, a stand-in
for a built web UI that `src/ui_tests.rs` embeds at compile time.
`crates/gglib-proxy/tests/fixtures/` is Rust source rather than data: `mod.rs`
declares seventeen modules, among them `common` and `sse`, and each of the
fifty-one `crates/gglib-proxy/tests/*.rs` integration binaries pulls them in
with a plain `mod fixtures;`. rustc resolves that against the directory of the
test file that declares it, at compile time — nothing reads a path at runtime,
and `CARGO_MANIFEST_DIR` is not involved. That is also why the directory has to
sit beside those test files specifically, not merely somewhere inside the crate.
