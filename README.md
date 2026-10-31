# badges

The files the shields.io badges in gglib's READMEs read. Nothing here is
written by hand: `.github/workflows/badges.yml` runs `scripts/write_badges.py`
after each successful CI or Coverage run of a push to `main`, which rewrites
this README and deletes any JSON file not listed below.

| File | What it is | Written after |
|---|---|---|
| `<crate>-tests.json` | Tests passed of tests run, from the crate's `rust-test-<crate>.txt` | CI |
| `<crate>-coverage.json` | Lines covered, from the crate's `<crate>-lcov.info` | Coverage |
| `<crate>-loc.json`, `<crate>-complexity.json` | Lines of Rust and their complexity, counted by scc | CI |
| `version.json` | The workspace version in `Cargo.toml` | CI |
| `status-light.svg`, `status-dark.svg` | The README's status card | `status-card.yml`, daily |

`<crate>` is each crate with badges in `crates/layers.toml`: gglib-core, gglib-db, gglib-download, gglib-gguf, gglib-bootstrap, gglib-app-services, gglib-hf, gglib-build-info, gglib-mcp, gglib-proxy, gglib-runtime, gglib-agent, gglib-engine-llama, gglib-broadcast, gglib-test-http, gglib-cli, gglib-axum, gglib-ffi.

A file reads `none` when the run's artifact held nothing for its crate, and
`pending` until the first run that writes it.
