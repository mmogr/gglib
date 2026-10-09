# Every target here is a command, not a file. The previous list named 15 of
# them, so `make test` and `make setup` were one stray directory away from
# being skipped as up-to-date.
.PHONY: help setup install uninstall build build-dev build-gui build-all build-tauri \
        test test-helper check fmt lint doc doc-check dev pre-commit release \
        lint-web typecheck-web deadcode-web test-web boundaries unused-deps enforce \
        bindings bindings-check \
        clean clean-gui clean-llama clean-db clean-all \
        check-deps check-deps-bootstrap check-rust \
        llama-install llama-install-auto llama-update llama-status llama-rebuild \
        run-serve run-proxy run-gui run-web

# Platform specific configuration
UNAME_S := $(shell uname -s)
ifeq ($(UNAME_S),Linux)
    export LIBSQLITE3_SYS_USE_PKG_CONFIG := 1
    # Fix Node.js/npm segfault on WSL2 (io_uring not fully supported by WSL2 kernel)
    export UV_USE_IO_URING := 0
endif

# Define cargo command that sources Rust environment if needed (for non-interactive shells like VS Code tasks)
# This is a portable solution that works on Linux/macOS/Windows
CARGO_ENV := $(shell if [ -f "$$HOME/.cargo/env" ]; then echo ". $$HOME/.cargo/env &&"; fi)
# Name rustup's shim explicitly rather than relying on PATH order.
#
# Sourcing ~/.cargo/env is not sufficient on its own: that script only prepends
# ~/.cargo/bin when it is *absent* from PATH, so if it is present but ranked
# below a standalone toolchain (Homebrew's `rust` formula installs one at
# /opt/homebrew/bin), the script is a silent no-op and the standalone compiler
# wins. Those do not honour rust-toolchain.toml, so the build quietly runs on
# an unpinned rustc — and only in the shells that skip ~/.zshrc, which is
# exactly the non-interactive case this block exists to handle.
CARGO_BIN := $(shell if [ -x "$$HOME/.cargo/bin/cargo" ]; then echo "$$HOME/.cargo/bin/cargo"; else echo cargo; fi)
CARGO := $(CARGO_ENV) $(CARGO_BIN)

# Release optimization flags live in [profile.release] in Cargo.toml, not here.
# They used to be exported as CARGO_PROFILE_RELEASE_LTO/_CODEGEN_UNITS with the
# same values the manifest already had; because the env vars take precedence,
# any future edit to Cargo.toml would have been silently ignored under `make`.

##@ Dependencies

# Bootstrap dependency check - runs WITHOUT requiring Rust compilation.
# It checks the toolchains a build needs, and then, when a gglib binary
# exists, hands over to `gglib config check-deps` for the rest of the list.
check-deps-bootstrap: ## Check the build toolchains, then the rest if gglib is built
	@chmod +x scripts/check-deps.sh
	@./scripts/check-deps.sh

# Check if Rust/Cargo is installed
check-rust: ## Verify Rust and Cargo are installed
	@if ! command -v cargo >/dev/null 2>&1; then \
		echo ""; \
		echo "╔════════════════════════════════════════════════════════════════╗"; \
		echo "║  ✗ Rust is not installed                                       ║"; \
		echo "╚════════════════════════════════════════════════════════════════╝"; \
		echo ""; \
		echo "Rust and Cargo are required to build and run gglib."; \
		echo "Install Rust: curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"; \
		exit 1; \
	fi

# The dependency check `setup` starts with. The list of what gglib needs is
# `gglib config check-deps`'s, in Rust, and the bash script holds only what
# has to be there before that command can be built.
check-deps: check-deps-bootstrap ## Check system dependencies

##@ Help

# Descriptions come from the `## ...` comment on each target, so this list can
# no longer drift from the targets themselves — which the hand-written version
# had, omitting check, fmt, lint, doc, dev, pre-commit, release and six others.
help: ## Show this help
	@echo "GGLib Makefile - Available targets:"
	@awk 'BEGIN {FS = ":.*##"} \
		/^##@/ { printf "\n\033[1m%s\033[0m\n", substr($$0, 5); next } \
		/^[a-zA-Z_-]+:.*?##/ { printf "  \033[36m%-22s\033[0m %s\n", $$1, $$2 }' $(MAKEFILE_LIST)

##@ Build and install

# Where `install` puts the binary, and so what `uninstall` removes. It is a
# plain copy: no `cargo install` ever ran, so `cargo uninstall` has nothing to
# remove.
INSTALLED_BIN := $$HOME/.cargo/bin/gglib

# Uses pre-built binary from target/release/ (built by build-tauri or cargo build)
install: ## Build and install gglib to ~/.cargo/bin/
	@echo "Installing gglib..."
	@mkdir -p "$$HOME/.cargo/bin"
	@cp target/release/gglib "$(INSTALLED_BIN)"
ifeq ($(UNAME_S),Darwin)
	@codesign --force --sign - "$(INSTALLED_BIN)"
endif
	@echo "✓ Installed gglib to ~/.cargo/bin/gglib"

uninstall: ## Uninstall gglib and remove local state
	@echo "⚠️  WARNING: This will uninstall gglib and remove:"
	@echo "  - Binary from ~/.cargo/bin"
	@echo "  - System configuration and database (~/Library/Application Support/gglib or ~/.local/share/gglib)"
	@echo "  - Local build artifacts (target/, node_modules/, etc.)"
	@echo "  (Note: Your downloaded models in ~/.local/share/llama_models will be PRESERVED)"
	@echo ""
	@printf "Remove local data/ directory? [y/N] "; \
	read REMOVE_DATA; \
	echo ""; \
	printf "Proceed with uninstall? [y/N] "; \
	read REPLY; \
	if [ "$$REPLY" = "y" ] || [ "$$REPLY" = "Y" ]; then \
		echo "Uninstalling binary..."; \
		rm -f "$(INSTALLED_BIN)"; \
		if [ "$$REMOVE_DATA" = "y" ] || [ "$$REMOVE_DATA" = "Y" ]; then \
			echo "Removing system data..."; \
			rm -rf "$$HOME/Library/Application Support/gglib" 2>/dev/null || true; \
			rm -rf "$$HOME/.local/share/gglib" 2>/dev/null || true; \
		else \
			echo "Preserving system data (config and database retained)"; \
		fi; \
		echo "Cleaning build artifacts..."; \
		$(CARGO) clean || true; \
		if [ -d node_modules ]; then rm -rf node_modules || true; fi; \
		if [ -d web_ui ]; then rm -rf web_ui || true; fi; \
		if [ -d src-tauri/gen ]; then rm -rf src-tauri/gen || true; fi; \
		if [ -d .llama ]; then rm -rf .llama || true; fi; \
		if [ -d .gglib-runtime ]; then rm -rf .gglib-runtime || true; fi; \
		if [ -d .python ]; then rm -rf .python || true; fi; \
		if [ -d .conda ]; then rm -rf .conda || true; fi; \
		if [ -d pids ]; then rm -rf pids || true; fi; \
		if [ -f .env ]; then rm -f .env || true; fi; \
		if [ "$$REMOVE_DATA" = "y" ] || [ "$$REMOVE_DATA" = "Y" ]; then \
			rm -rf data/ || true; \
		fi; \
		if [ -d .git ]; then \
			if [ "$$REMOVE_DATA" = "y" ] || [ "$$REMOVE_DATA" = "Y" ]; then \
				git clean -xffd || true; \
			else \
				git clean -xffd -e data/ || true; \
			fi; \
		fi; \
		if [ "$$REMOVE_DATA" = "y" ] || [ "$$REMOVE_DATA" = "Y" ]; then \
			echo "✓ Uninstall complete (including data/)"; \
		else \
			echo "✓ Uninstall complete (data/ preserved)"; \
		fi; \
	else \
		echo "Cancelled."; \
	fi

build: ## Build Rust CLI in release mode
	@echo "Building release binary..."
	$(CARGO) build --release

build-dev: ## Build Rust CLI in debug mode
	@echo "Building debug binary..."
	$(CARGO) build

# Build web UI frontend
build-gui: ## Build web UI frontend
	@echo "Building web UI frontend..."
	@if ! command -v npm >/dev/null 2>&1; then echo "Error: npm not found"; exit 1; fi
	@# `npm ci`, not `npm install`: a build installs what package-lock.json
	@# says and leaves the file alone. `npm install` rewrites it (npm 10
	@# drops its "libc" entries), and a modified lockfile stops a
	@# `git pull` that changes it. tests/setup_scripts.rs pins both lines.
	UV_USE_IO_URING=0 npm ci
	UV_USE_IO_URING=0 npm run build
	@echo "✓ Web UI built to web_ui/"

# Build everything (Rust + Web UI)
build-all: build-gui build ## Build Rust CLI and web UI
	@echo "✓ Built Rust CLI and Web UI"

##@ Rust checks

# Run all tests
test: ## Run Rust tests
	@echo "Running all tests..."
	$(CARGO) test

# The download accelerator's helper is Python, so `cargo test` does not run
# it. Its tests need a venv holding the packages of hf_xet_requirements.txt,
# which CI builds on every run and a machine has only if someone built it:
#   python3 -m venv target/helper-venv
#   target/helper-venv/bin/pip install -r crates/gglib-download/scripts/hf_xet_requirements.txt
# Without one this says so and passes, and it is not part of `pre-commit`.
HELPER_VENV ?= target/helper-venv
HELPER_DIR := crates/gglib-download/scripts

test-helper: ## Run the download helper's Python tests, if its venv is there
	@if [ -x "$(HELPER_VENV)/bin/python" ]; then \
		cd $(HELPER_DIR) && "$(abspath $(HELPER_VENV))/bin/python" -m unittest test_hf_xet_downloader; \
	else \
		echo "⚠ no venv at $(HELPER_VENV): skipping the download helper's tests (see the Makefile for how to build one)"; \
	fi

# Check code without building
check: ## Check Rust code without building
	@echo "Checking code..."
	$(CARGO) check

# Format code
fmt: ## Format Rust code
	@echo "Formatting code..."
	$(CARGO) fmt

# Run clippy
lint: ## Run clippy with warnings denied
	@echo "Running clippy linter..."
	@# `--all-targets --all-features`, matching ci.yml exactly. Without
	@# --all-targets clippy skips test code, so a lint error inside a
	@# `#[cfg(test)]` module passes `make pre-commit` and fails CI — which is
	@# precisely how a single-element loop reached main. A pre-commit target
	@# that claims to run everything CI requires has to actually run it.
	$(CARGO) clippy --all-targets --all-features -- -D warnings

# Generate and open documentation
doc: ## Generate and open documentation
	@echo "Generating documentation..."
	@# `--document-private-items` as in `doc-check` and CI, so a private
	@# intra-doc link resolves here exactly as it does there.
	$(CARGO) doc --document-private-items --open

# `export` rather than a command-prefix assignment, for the reason spelled out
# above the `bindings` target: `$(CARGO)` expands to `. $$HOME/.cargo/env &&
# …cargo`, so `RUSTDOCFLAGS=x $(CARGO) doc` prefixes the `.` builtin rather
# than cargo. That it works at all is an accident of POSIX — assignments before
# a *special* builtin persist into the shell — and it is not a rule worth
# resting a gate on. Written the way the rest of this file already writes it.
doc-check: export RUSTDOCFLAGS := -D warnings

doc-check: ## Build rustdoc with warnings denied, exactly as CI does
	@echo "Checking rustdoc..."
	@# The same invocation as ci.yml and release.yml. Every flag
	@# matters: `--document-private-items` is what makes these docs worth
	@# reading (most of this codebase is private), and it is also what the
	@# workspace's `private_intra_doc_links = "allow"` is predicated on.
	@# `--exclude gglib-app`: the one member whose build can need the Web UI
	@# (its `tauri::generate_context!()` embeds `frontendDist` when
	@# `custom-protocol` is on); see the note above `bindings`.
	$(CARGO) doc --workspace --no-deps --document-private-items --exclude gglib-app

##@ Frontend and architecture checks

lint-web: ## Run eslint over src/ (no warnings allowed)
	@echo "Linting web UI..."
	npm run lint -- --max-warnings 0

typecheck-web: ## Type-check src, tests and the build config
	@echo "Type-checking web UI..."
	npm run typecheck

deadcode-web: ## Find TypeScript files nothing imports, and undeclared imports
	@echo "Checking for dead files..."
	@# Neither tsc nor eslint can see this: a file nothing imports still
	@# compiles, and eslint judges one file at a time. Scoped to files and
	@# dependencies; the export-level classes are not gated yet.
	npm run deadcode

test-web: ## Run the frontend test suite
	@echo "Running frontend tests..."
	npm run test:run

# `boundaries`, `unused-deps` and `enforce` are what ci.yml runs: its
# `boundaries` job calls the first two and its `enforcement` job the third, so
# a check added to one of these recipes runs in CI and in `pre-commit` alike.
boundaries: ## Check crate boundaries
	@./scripts/check_boundaries.sh

# The Rust sibling of `deadcode-web`. cargo-shear is a tool of its own: CI
# installs a prebuilt binary, and `cargo binstall cargo-shear` or
# `brew install cargo-shear` does the same here. Building it with
# `cargo install` can need a newer rustc than rust-toolchain.toml pins.
unused-deps: ## Find dependencies no crate uses (needs cargo-shear)
	@echo "Checking for unused dependencies..."
	@$(CARGO) shear --version >/dev/null 2>&1 || { \
		echo "✗ cargo-shear is not installed: cargo binstall cargo-shear, or brew install cargo-shear"; \
		exit 1; \
	}
	@# `--deny-warnings`: an optional dependency nothing uses and an `ignored`
	@# entry that is no longer needed fail too.
	$(CARGO) shear --deny-warnings

# The architecture checks. This recipe is the only list of them: CONTRIBUTING
# and scripts/README point here and keep no list of their own.
enforce: ## Run the architecture enforcement checks
	@./scripts/check-tauri-commands.sh
	@./scripts/check-frontend-ipc.sh
	@./scripts/check_transport_branching.sh
	@./scripts/check_param_source_exhaustive.sh
	@# A setting that exists, is plumbed, is read, and is settable from nowhere
	@# compiles perfectly — the failure is an absence. tool_call_repair sat that
	@# way for months while `config settings show` printed it.
	@./scripts/check_settings_surfaces.sh
	@# `let _ = sqlx::query(…)` tolerates a locked database and a missing table
	@# on the same terms as the duplicate column its comment named. #796 shipped
	@# exactly that: an ALTER failed with "no such table", said nothing, and left
	@# every fresh install without the column until a second boot.
	@./scripts/check_swallowed_db_errors.sh
	@# `.unwrap_or(DEFAULT_CONTEXT_SIZE)` reads as a harmless default and turns
	@# "the user chose nothing" into "the user chose 4096", which outranks the
	@# fitted rung. The same construct shipped in #925, #926 and #934, each time
	@# found months later by reading. The construct is the tell.
	@./scripts/check_context_floor.sh
	@# The file-size check, once per language with that language's baseline.
	@# The 300-LOC budget is a guide for noticing a file that has taken on a
	@# second job, so this stops on growth and not on size: a file past the
	@# budget passes at its recorded size, and one that is a single concept
	@# has its row raised by hand (CONTRIBUTING, "File size").
	@./scripts/check_file_size.sh rust scripts/rust-complexity-baseline.txt
	@./scripts/check_file_size.sh ts scripts/ts-complexity-baseline.txt
	@./scripts/check_lint_inheritance.sh
	@# CI runs this too, but it cannot catch a break in ci.yml itself: GitHub
	@# starts no jobs at all in a workflow file it will not parse. Local is the
	@# only place that case gets caught before the push.
	@./scripts/check_workflow_yaml.sh
	@# ts-rs reads Rust types, not serde's behaviour, so two annotations stand
	@# between the generated bindings and a new class of lie: i64 becomes a
	@# `bigint` no `JSON.parse` can produce, and `skip_serializing_if` becomes a
	@# required nullable rather than an optional. Neither fails to compile.
	@./scripts/check_ts_bindings.sh
	@# The TypeScript README tables are written by hand. Apart from this, the
	@# only check on them is that the README exists and has its markers.
	@./scripts/check_readme_tables.py
	@# A link into docs/adr/ breaks when an ADR or a heading is renamed, and
	@# rustdoc does not follow a URL.
	@./scripts/check_adrs.py --check

##@ Bindings

BINDINGS_DIR := src/types/generated
BINDINGS_LOG := target/bindings-export.log

# An unqualified `--features ts-bindings` under `--workspace` reaches every
# package that defines it — measured, not assumed — and keeps reaching any
# crate that declares it later, which a hand-maintained package list would
# not.
#
# `--exclude gglib-app`: that package defines no `ts-bindings` and derives no
# `TS`, so it has nothing to export, and leaving it out keeps the export clear
# of the one member whose build can need the Web UI (its
# `tauri::generate_context!()` embeds `frontendDist` when `custom-protocol` is
# on). A `TS` derive added there would not be exported while it is excluded.
#
# `export` rather than a command-prefix assignment, and this is not style.
# `$(CARGO)` expands to `. $$HOME/.cargo/env && …cargo`, so
# `VAR=x $(CARGO) test` prefixes the `.` builtin, not cargo — and that
# assignment survives in macOS `/bin/sh` but is DROPPED by both bash and dash.
# `/bin/sh` is dash on ubuntu, so in CI the variable never arrived, ts-rs fell
# back to its default `./bindings`, and the `git diff` below then inspected an
# untouched tree and passed on every input.
bindings bindings-check: export TS_RS_EXPORT_DIR := $(CURDIR)/$(BINDINGS_DIR).new

# Regenerate the TypeScript the frontend imports, from the Rust that defines it.
#
# Generated into a sibling and swapped in only once both guards below pass, so
# no failure path leaves the tree without its bindings. It used to `rm -rf` the
# real directory first, before anything that could fail — a compile error
# anywhere in the workspace, or a Ctrl-C, left 178 committed files deleted with
# no message saying so, and `check_ts_bindings.sh` reported a clean run over
# the hole.
#
# The swap keeps the property that motivated emptying it: a type that stops
# deriving `TS` still returns as a deletion, because the whole directory is
# replaced rather than written over.
bindings: ## Regenerate src/types/generated from the Rust wire types
	@mkdir -p target
	@rm -rf $(BINDINGS_DIR).new
	@$(CARGO) test --workspace --exclude gglib-app --features ts-bindings export_bindings_ \
		> $(BINDINGS_LOG) 2>&1 || { rm -rf $(BINDINGS_DIR).new; cat $(BINDINGS_LOG); exit 1; }
	@ran=$$(grep -cE 'export_bindings_[a-z0-9_]+ \.\.\. ok' $(BINDINGS_LOG) || true); \
	 if [ "$$ran" -lt 1 ]; then \
		rm -rf $(BINDINGS_DIR).new; \
		echo "✗ no export tests ran — the ts-bindings feature did not take."; \
		cat $(BINDINGS_LOG); \
		exit 1; \
	 fi; \
	 if [ ! -d "$(BINDINGS_DIR).new" ]; then \
		echo "✗ $$ran export tests ran but $(BINDINGS_DIR).new does not exist —"; \
		echo "  TS_RS_EXPORT_DIR did not reach cargo. Check the shell it runs under."; \
		exit 1; \
	 fi; \
	 rm -rf $(BINDINGS_DIR); \
	 mv $(BINDINGS_DIR).new $(BINDINGS_DIR); \
	 echo "✓ bindings regenerated ($$ran types)"

# Regenerates first, then compares against HEAD — so this asks "do the
# committed bindings match the Rust?", which is the question that matters. A
# hand-edit of a generated file is not caught here so much as undone: the
# regeneration overwrites it, and if it was committed, the overwrite is the
# diff. Against HEAD rather than the index, so a staged edit cannot hide.
bindings-check: bindings ## Fail if the committed bindings are stale
	@git diff HEAD --exit-code -- $(BINDINGS_DIR) || { \
		echo ""; \
		echo "✗ $(BINDINGS_DIR) is out of date with the Rust."; \
		echo "  Run 'make bindings' and commit the result."; \
		exit 1; \
	}
	@# An orphan: a type deleted from Rust leaves its binding behind, and a
	@# regenerate-and-diff cannot see it, because nothing rewrites a file that
	@# no longer has a source. `git status` does, now that the directory is
	@# emptied first.
	@untracked=$$(git status --porcelain -- $(BINDINGS_DIR)); \
	 if [ -n "$$untracked" ]; then \
		echo "$$untracked"; \
		echo ""; \
		echo "✗ $(BINDINGS_DIR) has files git does not know about."; \
		echo "  Usually a type that was renamed or deleted in Rust. Commit or remove them."; \
		exit 1; \
	 fi
	@# The other place ts-rs writes: `<crate>/bindings` is its default, taken
	@# whenever TS_RS_EXPORT_DIR does not reach cargo — which a scoped
	@# `cargo test export_bindings_` run does not carry. The gate above looks
	@# only at $(BINDINGS_DIR), so a second copy of every generated type was
	@# once committed beside the crates without anything noticing. .gitignore
	@# stops them being tracked; this stops them being written and left.
	@strays=$$(find crates -type d -name bindings -not -path '*/node_modules/*' 2>/dev/null); \
	 if [ -n "$$strays" ]; then \
		echo "$$strays"; \
		echo ""; \
		echo "✗ ts-rs wrote to its default directory instead of $(BINDINGS_DIR)."; \
		echo "  TS_RS_EXPORT_DIR did not reach cargo. Remove these and re-run"; \
		echo "  'make bindings', which exports through the staging directory."; \
		exit 1; \
	 fi
	@echo "✓ bindings match the Rust they are generated from"

##@ Cleaning

# Clean build artifacts
clean: ## Remove Rust build artifacts
	@echo "Cleaning build artifacts..."
	$(CARGO) clean
	@echo "✓ Removed target/ directory"

# Clean web UI build
clean-gui: ## Remove web UI build and node_modules
	@echo "Cleaning web UI build artifacts..."
	@if [ -d web_ui ]; then \
		rm -rf web_ui && echo "✓ Removed web_ui/ directory"; \
	else \
		echo "⚠ web_ui/ directory not found"; \
	fi
	@if [ -d node_modules ]; then \
		rm -rf node_modules && echo "✓ Removed node_modules/ directory"; \
	else \
		echo "⚠ node_modules/ directory not found"; \
	fi

# Clean llama.cpp installation
clean-llama: ## Remove llama.cpp installation
	@echo "Removing llama.cpp installation..."
	@if [ -d .llama ]; then \
		rm -rf .llama && echo "✓ Removed .llama/ directory"; \
	else \
		echo "⚠ .llama/ directory not found"; \
	fi

# Clean database files
clean-db: ## Remove database files
	@echo "Removing database files..."
	@if [ -d data ]; then \
		rm -rf data && echo "✓ Removed data/ directory"; \
	else \
		echo "⚠ data/ directory not found"; \
	fi

# Nuclear option - remove everything
clean-all: ## Remove everything (git clean -xffd)
	@echo "⚠️  WARNING: This will remove ALL untracked files and build artifacts!"
	@printf "Are you sure? [y/N] "; \
	read REPLY; \
	if [ "$$REPLY" = "y" ] || [ "$$REPLY" = "Y" ]; then \
		git clean -xffd; \
		echo "✓ Repository cleaned"; \
	else \
		echo "Cancelled."; \
	fi

##@ llama.cpp

# The gglib a llama.cpp target runs: the release build, else the debug build,
# else one `cargo run` builds first. Expanded when a recipe runs, so it sees a
# binary built earlier in the same `make`.
GGLIB = $(or $(addprefix ./,$(firstword $(wildcard target/release/gglib target/debug/gglib))),$(CARGO) run -p gglib-cli --)

# llama.cpp management targets
llama-install: ## Install llama.cpp (manual)
	@echo "Installing llama.cpp (manual)..."
	@$(GGLIB) config llama install

llama-install-auto: ## Install llama.cpp (auto-detect GPU)
	@echo "Installing llama.cpp with auto-detected GPU support..."
	@scripts/install-llama.sh

llama-update: ## Update llama.cpp
	@echo "Updating llama.cpp..."
	@$(GGLIB) config llama update

llama-status: ## Show llama.cpp status
	@$(GGLIB) config llama status

llama-rebuild: clean-llama llama-install-auto ## Reinstall llama.cpp from scratch
	@echo "✓ llama.cpp rebuilt"

##@ Running

# Quick run targets
run-serve: ## Run gglib serve (release mode)
	@echo "Running gglib serve (release mode)..."
	$(CARGO) run -p gglib-cli --release -- serve $(if $(ID),$(ID),1)

run-proxy: ## Run gglib proxy (release mode)
	@echo "Starting gglib proxy (release mode)..."
	$(CARGO) run -p gglib-cli --release -- proxy

# Run desktop GUI
run-gui: ## Run the desktop GUI
	@echo "Starting desktop GUI..."
	$(CARGO) run -p gglib-cli -- gui

# Run web server
#
# No PORT: `gglib web` ensures the daemon and prints its URL, and the daemon's
# port is a fixed loopback constant by design so every client can find the one
# daemon without configuration. This target passed `--port $(PORT)`, which the
# command has never accepted — `make run-web PORT=9999` failed with a clap
# error.
run-web: ## Run the web server
	@echo "Starting web server..."
	$(CARGO) run -p gglib-cli -- web

##@ Tauri

# Build Tauri desktop app (production)
# Uses "Manual Build + Bundle" strategy to avoid double compilation:
# 1. Build frontend (vite)
# 2. Build both CLI and Tauri app in a single cargo invocation (shared deps compile once)
# 3. Bundle the already-built binary into platform installers
build-tauri: ## Build Tauri desktop app
	@echo "Building Tauri desktop app..."
	@if ! command -v npm >/dev/null 2>&1; then echo "Error: npm not found"; exit 1; fi
	@rm -f target/release/bundle/dmg/*.dmg 2>/dev/null || true
	UV_USE_IO_URING=0 npm ci
	# Step A: Build frontend
	UV_USE_IO_URING=0 npm run build:tauri
	# Step B: Unified cargo build - both CLI and Tauri app share dependency compilation
	# custom-protocol is required for Tauri to serve bundled frontend assets via
	# its asset protocol.  Without it the WebView falls back to devUrl and shows
	# a blank white screen in production.
	$(CARGO) build --release -p gglib-cli -p gglib-app --features gglib-app/custom-protocol
	# Step C: Bundle the already-built binary into platform installers
	# On Linux: use --bundles deb,rpm to avoid AppImage issues on Arch.
	# linuxdeploy's embedded strip fails on Arch due to RELR relocations (linuxdeploy#272).
	# NO_STRIP=1 is a linuxdeploy-supported knob that avoids the failure by skipping stripping.
	# On macOS: use defaults to produce .app bundle.
	@if [ "$(UNAME_S)" = "Linux" ]; then \
		NO_STRIP=1 UV_USE_IO_URING=0 npm run tauri:bundle -- --bundles deb,rpm; \
	else \
		npm run tauri:bundle; \
	fi
	@echo "✓ Tauri app built to target/release/gglib-app"

##@ Workflows

# Full setup from scratch
# Note: build-tauri builds the web UI, gglib-app and gglib-cli, so nothing
# else here builds; install just copies the binary
# llama-install-auto runs last and is REQUIRED to succeed: the command it
# runs refuses a machine with no GPU, and one whose GPU lacks what building
# for it needs, sooner than produce a CPU-only llama-server.
setup: check-deps build-tauri install ## Full setup (check deps + build + install)
	@echo "Configuring models directory (press Enter to accept the default)"
	@./target/release/gglib config models-dir prompt
	@# Optional accelerator. The command already refuses to fail — it skips
	@# without a terminal and reports a failed provision as a skipped step —
	@# but setup must not break over an optional extra, so belt and braces.
	@./target/release/gglib config fast-downloads prompt || true
	@echo "✓ Core setup complete!"
	@$(MAKE) llama-install-auto

# Development workflow
dev: fmt lint test ## Format, lint and test
	@echo "✓ Development checks passed"

# Pre-commit checks.
#
# These are exactly the jobs ci-success requires, in the same order: fmt,
# clippy, cargo test, the eslint/tsc gate, the unimported-file check, the
# frontend suite, the boundary script, the unused-dependency check, the
# architecture scripts, the binding staleness gate and rustdoc. It used to be
# `fmt lint check test` — all Cargo — which meant a clean local run could
# still fail CI on eslint, on a type error in a test file, or on any of the
# five shell checks.
#
# `bindings-check` earns its place for the same reason `doc-check` did: the
# `test` job runs it, so a stale binding fails CI, and `enforce`'s
# `check_ts_bindings.sh` cannot stand in — that reads annotations, and a
# missing annotation regenerates consistently and leaves this diff clean.
# Without it, adding a Rust wire field and forgetting `make bindings` passed
# a target whose help text reads "everything CI requires" and then cost a
# full Rust CI leg to discover.
pre-commit: fmt lint test lint-web typecheck-web deadcode-web test-web boundaries unused-deps enforce bindings-check doc-check ## Run everything CI requires
	@echo "✓ All pre-commit checks passed"

# Release workflow
release: clean test lint build-all install ## Clean, test, lint, build and install
	@echo "✓ Release build and install complete"
