# GGLib Helper Scripts

Helper scripts for development, CI enforcement, documentation and releases.

A check's header comment is its specification. This file says which script is
which and what runs it. Of the checks, only the file-size check has a section
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

The file-size check, run once per language. Its 300-line budget is a guide,
not a limit: it is there so that a file which has taken on a second job is
noticed, and CONTRIBUTING's [File size](../CONTRIBUTING.md#file-size) says
what follows.

A file already over the budget is recorded in that language's baseline at its
size and may shrink freely. The check stops when a file is longer than its
row, and when a file with no row crosses the budget. It is a ratchet rather
than a threshold because well over a hundred Rust files and more than a dozen
TypeScript ones are over the budget, many of them one thing: a gate on size
would fail on every commit and be switched off within a day, which is how a
check becomes decorative.

When it stops, the question is whether the file is still one thing. One that
has taken on a second job is split at that seam. One that is a single concept
stays whole, and its row is raised, or added, by hand, so the diff shows the
number going up. A sibling file added to get under the number is the one
answer that is wrong.

`--update` rewrites every row to the tree's sizes. On a tree that passes, it
can only lower a row or drop one, and the check says when a row is above its
file. On a tree that fails it records every growth at once, which is why a
deliberate growth is one row edited by hand.

```bash
# crates/ and src-tauri/
./scripts/check_file_size.sh rust scripts/rust-complexity-baseline.txt [--update]
# src/**/*.{ts,tsx,css}, except src/types/generated/
./scripts/check_file_size.sh ts scripts/ts-complexity-baseline.txt [--update]
```

## Other scripts

| Script | Purpose | Run by |
|--------|---------|--------|
| `check-deps.sh` | What has to be installed before there is a gglib binary to ask: the Rust and Node toolchains, git, pkg-config, CMake and the C/C++ toolchain, and under WSL2 the kernel setting that crashes npm. It prints how to install what is missing. When a gglib binary exists it then runs `gglib config check-deps`, which holds the rest of the list (a GPU runtime and what building for it needs, and on Linux the libraries the desktop app links), and exits with that command's status | `make check-deps`, and `make setup` through it |
| `install-llama.sh` | Runs `gglib config llama install`, which picks the acceleration itself (Metal, CUDA or Vulkan) and refuses a machine with none of them. With no terminal it answers the command's "Continue?" with yes | `make llama-install-auto`, and `make setup` through it |
| `generate_submodule_readmes.sh` | Writes a README stub wherever a source directory lacks one | By hand |
| `split_test_output.py` | Cuts one `cargo test` run into the per-crate files the badges read | `ci.yml`'s `test` job |
| `check_issue_form_mapping.mjs` | The issue form's field ids are the ones `issue-labels.yml` maps to labels | `check-issue-form.yml` |
| `sync_versions.py` | Copies the workspace version into `package.json` | `bump-version.yml` |
| `lock_changes.py` | Lists what moved between two copies of a lockfile | `update-deps.yml` |
| `backtest_label_heuristics.sh` | Measures the pull-request labelling heuristics against merged pull requests | By hand |
| `macos-install.command`, `MACOS-README.txt` | The double-click installer in the macOS release bundle and its instructions: it clears the quarantine attribute and offers to move the app to `/Applications` | `release.yml` copies both into the bundle |
| `experiments/` | The measurements that ADRs, `docs/sampling.md` and doc comments cite, kept so that they can be run again | By hand |

`badges.yml` inlines its own badge generation and invokes no script here.
