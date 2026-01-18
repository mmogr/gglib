.PHONY: help install build test clean clean-llama clean-all run-serve run-proxy check fmt lint doc build-gui build-tauri build-all check-deps check-rust llama-install-auto

# Environment Variables
export RUST_MIN_STACK := 16777216

# Platform specific configuration
UNAME_S := $(shell uname -s)
ifeq ($(UNAME_S),Linux)
    export LIBSQLITE3_SYS_USE_PKG_CONFIG := 1
endif

# Define cargo command that sources Rust environment if needed (for non-interactive shells like VS Code tasks)
# This is a portable solution that works on Linux/macOS/Windows
CARGO_ENV := $(shell if [ -f "$$HOME/.cargo/env" ]; then echo ". $$HOME/.cargo/env &&"; fi)
CARGO := $(CARGO_ENV) cargo

# Cargo Optimization Flags
export CARGO_PROFILE_RELEASE_LTO := thin
export CARGO_PROFILE_RELEASE_CODEGEN_UNITS := 16

# Bootstrap dependency check - runs WITHOUT requiring Rust compilation
check-deps-bootstrap:
	@chmod +x scripts/check-deps.sh
	@./scripts/check-deps.sh

# Check if Rust/Cargo is installed
check-rust:
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

# Comprehensive dependency check using Rust
check-deps: check-deps-bootstrap
	@echo ""
	@echo "Running detailed dependency verification..."
	@$(CARGO) run -p gglib-cli --quiet -- check-deps

# Default target
help:
	@echo "GGLib Makefile - Available targets:"
	@echo "  make setup                - Full setup (check deps + build + install)"
	@echo "  make install              - Build and install gglib to ~/.cargo/bin/"
	@echo "  make uninstall            - Uninstall gglib and clean everything"
	@echo "  make build                - Build Rust CLI in release mode"
	@echo "  make build-dev            - Build Rust CLI in debug mode"
	@echo "  make build-gui            - Build web UI frontend"
	@echo "  make build-tauri          - Build Tauri desktop app"
	@echo "  make test                 - Run all tests"
	@echo "  make clean                - Remove build artifacts"
	@echo "  make clean-gui            - Remove web UI build"
	@echo "  make clean-llama          - Remove llama.cpp installation"
	@echo "  make clean-db             - Remove database files"
	@echo "  make clean-all            - Remove everything (git clean -xffd)"
	@echo "  make llama-install-auto   - Install llama.cpp (auto-detect GPU)"
	@echo "  make run-serve            - Run gglib serve (release mode)"
	@echo "  make run-proxy            - Run gglib proxy (release mode)"
	@echo "  make run-gui              - Run desktop GUI"
	@echo "  make run-web              - Run web server"

# Build & Install
install:
	@echo "Installing gglib..."
	$(CARGO) install --path crates/gglib-cli

uninstall:
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
		$(CARGO) uninstall gglib || true; \
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
		if [ -d .conda ]; then rm -rf .conda || true; fi; \
		if [ -d pids ]; then rm -rf pids || true; fi; \
		if [ -f package-lock.json ]; then rm -f package-lock.json || true; fi; \
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

build:
	@echo "Building release binary..."
	$(CARGO) build --release

build-dev:
	@echo "Building debug binary..."
	$(CARGO) build

# Build web UI frontend
build-gui:
	@echo "Building web UI frontend..."
	@if ! command -v npm >/dev/null 2>&1; then echo "Error: npm not found"; exit 1; fi
	npm install
	npm run build
	@echo "✓ Web UI built to web_ui/"

# Build everything (Rust + Web UI)
build-all: build-gui build
	@echo "✓ Built Rust CLI and Web UI"

# Run all tests
test:
	@echo "Running all tests..."
	$(CARGO) test

# Check code without building
check:
	@echo "Checking code..."
	$(CARGO) check

# Format code
fmt:
	@echo "Formatting code..."
	$(CARGO) fmt

# Run clippy
lint:
	@echo "Running clippy linter..."
	$(CARGO) clippy -- -D warnings

# Generate and open documentation
doc:
	@echo "Generating documentation..."
	$(CARGO) doc --open

# Clean build artifacts
clean:
	@echo "Cleaning build artifacts..."
	$(CARGO) clean
	@echo "✓ Removed target/ directory"

# Clean web UI build
clean-gui:
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
clean-llama:
	@echo "Removing llama.cpp installation..."
	@if [ -d .llama ]; then \
		rm -rf .llama && echo "✓ Removed .llama/ directory"; \
	else \
		echo "⚠ .llama/ directory not found"; \
	fi

# Clean database files
clean-db:
	@echo "Removing database files..."
	@if [ -d data ]; then \
		rm -rf data && echo "✓ Removed data/ directory"; \
	else \
		echo "⚠ data/ directory not found"; \
	fi

# Nuclear option - remove everything
clean-all:
	@echo "⚠️  WARNING: This will remove ALL untracked files and build artifacts!"
	@printf "Are you sure? [y/N] "; \
	read REPLY; \
	if [ "$$REPLY" = "y" ] || [ "$$REPLY" = "Y" ]; then \
		git clean -xffd; \
		echo "✓ Repository cleaned"; \
	else \
		echo "Cancelled."; \
	fi

# llama.cpp management targets
llama-install:
	@echo "Installing llama.cpp (manual)..."
	@if [ -f "./target/release/gglib" ]; then ./target/release/gglib llama install; \
	elif [ -f "./target/debug/gglib" ]; then ./target/debug/gglib llama install; \
	else $(CARGO) run -p gglib-cli -- llama install; fi

llama-install-auto:
	@echo "Installing llama.cpp with auto-detected GPU support..."
	@scripts/install-llama.sh

llama-update:
	@echo "Updating llama.cpp..."
	@if [ -f "./target/release/gglib" ]; then ./target/release/gglib llama update; \
	elif [ -f "./target/debug/gglib" ]; then ./target/debug/gglib llama update; \
	else $(CARGO) run -p gglib-cli -- llama update; fi

llama-status:
	@if [ -f "./target/release/gglib" ]; then ./target/release/gglib llama status; \
	elif [ -f "./target/debug/gglib" ]; then ./target/debug/gglib llama status; \
	else $(CARGO) run -p gglib-cli -- llama status; fi

llama-rebuild: clean-llama llama-install-auto
	@echo "✓ llama.cpp rebuilt"

# Quick run targets
run-serve:
	@echo "Running gglib serve (release mode)..."
	$(CARGO) run -p gglib-cli --release -- serve $(if $(ID),$(ID),1)

run-proxy:
	@echo "Starting gglib proxy (release mode)..."
	$(CARGO) run -p gglib-cli --release -- proxy

# Run desktop GUI
run-gui:
	@echo "Starting desktop GUI..."
	$(CARGO) run -p gglib-cli -- gui

# Run web server
run-web:
	@echo "Starting web server..."
	$(CARGO) run -p gglib-cli -- web $(if $(PORT),--port $(PORT),)

# Build Tauri desktop app (production)
build-tauri:
	@echo "Building Tauri desktop app..."
	@if ! command -v npm >/dev/null 2>&1; then echo "Error: npm not found"; exit 1; fi
	@rm -f src-tauri/target/release/bundle/dmg/*.dmg 2>/dev/null || true
	npm install
	# On Linux: use --bundles deb,rpm to avoid AppImage issues on Arch.
	# linuxdeploy's embedded strip fails on Arch due to RELR relocations (linuxdeploy#272).
	# NO_STRIP=1 is a linuxdeploy-supported knob that avoids the failure by skipping stripping.
	# On macOS: use defaults to produce .app bundle.
	# Source Rust environment for npm subshell (needed for tauri to find cargo)
	@if [ "$(UNAME_S)" = "Linux" ]; then \
		if [ -f "$$HOME/.cargo/env" ]; then . "$$HOME/.cargo/env"; fi; \
		NO_STRIP=1 npm run tauri:build -- --bundles deb,rpm; \
	else \
		if [ -f "$$HOME/.cargo/env" ]; then . "$$HOME/.cargo/env"; fi; \
		npm run tauri:build; \
	fi
	@echo "✓ Tauri app built to src-tauri/target/release/gglib-gui"

# Full setup from scratch
setup: check-deps build-gui build-tauri install
	@echo "Configuring models directory (press Enter to accept the default)"
	@$(CARGO) run -p gglib-cli -- config models-dir prompt
	@echo "✓ Core setup complete!"
	@$(MAKE) llama-install-auto || echo "⚠ llama.cpp installation failed (optional)"

# Development workflow
dev: fmt lint test
	@echo "✓ Development checks passed"

# Pre-commit checks
pre-commit: fmt lint check test
	@echo "✓ All pre-commit checks passed"

# Release workflow
release: clean test lint build-all install
	@echo "✓ Release build and install complete"
