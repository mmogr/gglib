# GGLib Helper Scripts

Helper scripts for development, CI enforcement, documentation and releases.

A check's header comment is its specification. This file says which script is
which and what runs it. Of the checks, only the file-size ratchet has a section
here.

## Architecture checks

CI runs these through `make boundaries` and `make enforce`, the targets
`make pre-commit` runs. The `enforce` recipe in the [Makefile](../Makefile)
lists every check it runs, and this file keeps no second list.

| Script | Purpose | Run by |
|--------|---------|--------|
| `check_boundaries.sh` | The crate dependency rules of CONTRIBUTING's [Crate Boundaries](../CONTRIBUTING.md#crate-boundaries) that a crate's direct dependencies show, the `gglib-bootstrap` source guard, and `check_readmes.sh --strict` | `make boundaries`, in `ci.yml`'s `boundaries` job |
| `check_readmes.sh` | README coverage: each directory below a crate's `src/`, below `src-tauri/src/` and below the TypeScript `src/` (`src/types/generated/` excepted) has a README with its `module-docs` markers | `check_boundaries.sh`, with `--strict` |
| The checks in the `enforce` recipe | The repository rules that neither the compiler nor a linter checks | `make enforce`, in `ci.yml`'s `enforcement` job |

### `check_file_size.sh`

The file-size ratchet, run once per language. A file already over the 300-LOC
budget is recorded in that language's baseline at its size and may shrink
freely; growing it fails. A file not in the baseline may not cross the line at
all.

A ratchet rather than a threshold because a threshold could not be switched on:
well over a hundred Rust files and more than a dozen TypeScript ones are
already over. A gate that fails on every commit gets switched off within a day,
which is how a constraint becomes decorative.

`--update` rewrites the baseline to the tree's sizes. Use it when a file
legitimately grew and the growth is the point: the diff then shows the number
going up. On a tree that passes, it can only lower a row or drop one, and the
check says when a row is above its file.

```bash
# crates/ and src-tauri/
./scripts/check_file_size.sh rust scripts/rust-complexity-baseline.txt [--update]
# src/**/*.{ts,tsx,css}, except src/types/generated/
./scripts/check_file_size.sh ts scripts/ts-complexity-baseline.txt [--update]
```

`check_rust_complexity.sh` and `check_file_complexity.sh` run those two
commands, under the names that module docs cite.

## Other scripts

| Script | Purpose | Run by |
|--------|---------|--------|
| `check-deps.sh` | The system dependencies of a build, checked before anything compiles: the Rust, Node and Python toolchains, git, the C/C++ toolchain and CMake that llama.cpp builds with, a GPU runtime and what building for it needs, and on Linux the libraries the desktop app links. It prints how to install what is missing | `make check-deps`, and `make setup` through it |
| `install-llama.sh` | Picks the acceleration llama.cpp is built with (Metal, CUDA or Vulkan) and runs `gglib config llama install` with it. Where it finds none of the three it chooses `--cpu-only`, which that command does not accept, and nothing is installed | `make llama-install-auto`, and `make setup` through it |
| `generate_submodule_readmes.sh` | Writes a README stub wherever a source directory lacks one | By hand |
| `split_test_output.py` | Cuts one `cargo test` run into the per-crate files the badges read | `ci.yml`'s `test` job |
| `check_issue_form_mapping.mjs` | The issue form's field ids are the ones `issue-labels.yml` maps to labels | `check-issue-form.yml` |
| `sync_versions.py` | Copies the workspace version into `package.json` | `bump-version.yml` |
| `lock_changes.py` | Lists what moved between two copies of a lockfile | `update-deps.yml` |
| `backtest_label_heuristics.sh` | Measures the pull-request labelling heuristics against merged pull requests | By hand |
| `macos-install.command`, `MACOS-README.txt` | The double-click installer in the macOS release bundle and its instructions: it clears the quarantine attribute and offers to move the app to `/Applications` | `release.yml` copies both into the bundle |
| `experiments/` | The measurements that ADRs, `docs/sampling.md` and doc comments cite, kept so that they can be run again | By hand |

`badges.yml` inlines its own badge generation and invokes no script here.
