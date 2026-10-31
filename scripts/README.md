# GGLib Helper Scripts

Helper scripts for development, CI enforcement, documentation and releases.

A check's header comment is its specification. This file says which script is
which and what runs it. Of the checks, only the file-size check has a section
here.

## Architecture checks

CI runs these through `make boundaries`, `make enforce` and
`make adr-readings`, the targets `make pre-commit` runs. The `enforce` recipe
in the [Makefile](../Makefile) lists every check it runs, and this file keeps
no second list.

| Script | Purpose | Run by |
|--------|---------|--------|
| `check_boundaries.sh` | The crate dependency rules of CONTRIBUTING's [Crate Boundaries](../CONTRIBUTING.md#crate-boundaries) that a crate's direct dependencies show, the `gglib-bootstrap` source guard, and `check_readmes.sh --strict` | `make boundaries`, in `ci.yml`'s `boundaries` job |
| `check_phone_closure.sh` | The phone crates' rule in CONTRIBUTING's [Crate Boundaries](../CONTRIBUTING.md#crate-boundaries): no crate the Makefile's `PHONE_CRATES` names reaches a desktop-only package in its `aarch64-apple-ios` dependency closure. It reads `cargo tree` and needs no iOS toolchain | `make boundaries`, in `ci.yml`'s `boundaries` job |
| `check_readmes.sh` | README coverage: each directory below a crate's `src/`, below `src-tauri/src/` and below the TypeScript `src/` (`src/types/generated/` excepted) has a README with its `module-docs` markers | `check_boundaries.sh`, with `--strict` |
| The checks in the `enforce` recipe | The repository rules that neither the compiler nor a linter checks | `make enforce`, in `ci.yml`'s `enforcement` job |
| `check_adr_readings.sh` | Each reading an ADR's criterion names, in `contracts/upstream/llama/observed.json`, against its hand-kept approval in `approvals.json`, and each quoted criterion against its ADR | `make adr-readings`, in `ci.yml`'s `adr-readings` job (`ADR criterion moved`) |

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

## The Swift framework (`apple/`)

gglib-ffi's framework and its Swift binding. Each script's header says what it
does and why; CONTRIBUTING's [The Swift framework](../CONTRIBUTING.md#the-swift-framework)
says how they are used. All but `generate-swift.sh` and `ffi-inputs-changed.sh`
need macOS with Xcode.

| Script | Purpose | Run by |
|--------|---------|--------|
| `apple/generate-swift.sh` | Writes the Swift binding, the C header and the `framework module GglibFFI` modulemap out of a built gglib-ffi library, with the crate's own `uniffi-bindgen`, and refuses when `uniffi.toml` names another module | `make ffi-binding`, and `build-xcframework.sh` |
| `apple/build-xcframework.sh` | Builds gglib-ffi for `aarch64-apple-ios`, `aarch64-apple-ios-sim` and `aarch64-apple-darwin` at a deployment floor of 26.0, wraps each as `GglibFFI.framework` (shallow on iOS, versioned on macOS) and assembles `target/apple/GglibFFI.xcframework`, then runs `check-slices.sh` | `make ffi-xcframework`, `make ffi-xcframework-fast` |
| `apple/check-slices.sh` | Each of the three slices is an arm64 dynamic library with the install name an app finds it at, the platform and floor it claims, every function its header declares, and the bundle, modulemap and Info.plist its platform loads | `build-xcframework.sh` |
| `apple/swift-smoke.sh` | Compiles the generated Swift in a scratch Swift package against the macOS slice and calls `build_info()` | `make ffi-smoke` |
| `apple/package-release.sh` | Packages a built framework and its Swift binding as the two release assets, `GglibFFI-<version>.xcframework.zip` (the XCFramework at the archive's root) and `GglibFFI-swift-<version>.zip` (the Swift, header, modulemap and a `binding.json` naming what `build_info()` reports), and refuses when that Swift is not the committed copy. Then checks them as an app meets them: the archive's root, the slices after an unzip, a Swift package taking the archive as its binary target, and that `swift package compute-checksum` is the SHA-256 `SHA256SUMS` lists | `release.yml`'s `build-phone` job |
| `apple/ffi-inputs-changed.sh` | Whether a change reaches the files `ffi-apple` builds from: gglib-ffi's workspace dependency closure, read from `cargo tree`, and the files the build reads | `ci.yml`'s `ffi-inputs` job |

## Other scripts

| Script | Purpose | Run by |
|--------|---------|--------|
| `check-deps.sh` | What has to be installed before there is a gglib binary to ask: the Rust and Node toolchains, git, pkg-config, CMake and the C/C++ toolchain, and under WSL2 the kernel setting that crashes npm. It prints how to install what is missing. When a gglib binary exists it then runs `gglib config check-deps`, which holds the rest of the list (a GPU runtime and what building for it needs, and on Linux the libraries the desktop app links), and exits with that command's status | `make check-deps`, and `make setup` through it |
| `uninstall.sh` | Removes gglib after asking three times, No each time by default: the installed binary and this checkout's build output; the data folder `gglib config paths` names, what an older build left in the checkout, and the desktop app with its caches and login item; then the models directory. Only the default data folder goes whole; one named by `GGLIB_DATA_DIR` loses just the names gglib writes there, and one that is the home folder, or above it or the checkout, is left alone and named. The models directory is deleted whole on a yes unless that would take the home folder, the checkout or listed data with it. Before data or models go it asks the daemon to stop, and refuses while the desktop app runs or, with no binary to ask, while any gglib does. A Homebrew app bundle and any other `gglib` on `PATH` are named and left to their package manager | `make uninstall` |
| `install-engine.sh` | `install-engine.sh llama\|sd` runs `gglib config <engine> install`, which installs the engine's pinned release: the download where this machine has one, and a source build for the acceleration it detects where it has none. With no terminal it answers a source build's "Continue?" with yes | `make llama-install-auto`, `make sd-install` and `make setup` |
| `generate_submodule_readmes.sh` | Writes a README stub wherever a source directory lacks one | By hand |
| `split_test_output.py` | Cuts one `cargo test` run into the per-crate files the badges read | `ci.yml`'s `test` job |
| `check_issue_form_mapping.mjs` | The issue form's field ids are the ones `issue-labels.yml` maps to labels | `check-issue-form.yml` |
| `promote.py` | Turns main's release candidate X.Y.Z-rc.N into X.Y.Z in `Cargo.toml`, `package.json` and the release-please manifest, and heads `CHANGELOG.md` with the stable version. `--self-test` runs its fixtures | `promote.yml`; `make enforce` runs the self-test |
| `check_rc_promotion.sh` | A stable version must be its last release candidate with only the version changed, `CHANGELOG.md` aside | `promote.yml`, `release.yml` |
| `build_cli_deb.sh` | Packages the Linux CLI binary as a `.deb` | `release.yml` |
| `build_apt_repo.sh` | Builds and signs the apt repository served at `/apt` on the Pages site | `release.yml` |
| `render_aur.sh` | Fills the `packaging/aur` PKGBUILD templates with a release's version and checksums | `release.yml` |
| `render_homebrew.sh` | Fills the `packaging/homebrew` formula and cask with a release's version and checksums | `release.yml` |
| `lock_changes.py` | Lists what moved between two copies of a lockfile | `update-deps.yml` |
| `llama_pin.sh` | Prints llama.cpp's pin, the tag `PINNED_LLAMA_RELEASE` names in `crates/gglib-core/src/domain/engine_pins.rs`, by the sed the Engine pins workflow reads it with | The `pinned-engines` action, which `ci.yml`'s CLI Check legs and `engine-pins.yml` run; the `llama-xcframework` action and the two `apple/` scripts below |
| `apple/build-llama-xcframework.sh` | Builds llama.cpp's own `llama.xcframework` at the pin for the phone engine: upstream's `build-xcframework.sh`, run unchanged for `ios-device`, `ios-sim` and `macos` in a shallow clone of the tag, into `target/apple/llama-<pin>/` with a zip of it and `pin.txt`. A folder already built at the pin is kept | `make phone-llama`; the `llama-xcframework` action, which `ci.yml`'s `phone-llama` job runs |
| `apple/check-llama-xcframework.sh` | A `llama.xcframework` has the iPhone, simulator and Mac slices, each a dynamic library for its platform and architectures with `llama.h`, and every architecture exports the `llama.h` functions the phone engine calls; its list of them is every `llama_*` function `crates/gglib-engine-llama/csrc/gglib_llama.c` calls, no more and no fewer | `make phone-llama`; the `llama-xcframework` action, before it saves its cache entry |
| `engine_pin_take.sh` | Takes an Engine pins recording job's handover (`record`'s, or on a llama.cpp bump `record-inproc`'s, which carries it on) into the next job's checkout as data: only regular files at the pin's path and under `contracts/upstream/`, with `approvals.json` kept from the checkout | `engine-pins.yml` |
| `engine_pin_body.sh` | Writes an engine pin pull request's body: the ADR readings the new `contracts/upstream` recording moved against `approvals.json`, what else it saw, whether the in-process engine was recorded and rendered and replied as llama-server did, tier 3's probes, and the upstream commits that touch a file an ADR cites | `engine-pins.yml` |
| `backtest_label_heuristics.sh` | Measures the pull-request labelling heuristics against merged pull requests | By hand |
| `macos-install.command`, `MACOS-README.txt` | The double-click installer in the macOS release bundle and its instructions: it clears the quarantine attribute and offers to move the app to `/Applications` | `release.yml` copies both into the bundle |
| `experiments/` | The measurements that ADRs, `docs/sampling.md` and doc comments cite, kept so that they can be run again | By hand |

`badges.yml` inlines its own badge generation and invokes no script here.
