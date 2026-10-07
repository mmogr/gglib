#!/usr/bin/env bash

# Bootstrap dependency checker for gglib
#
# Checks what has to be on the machine BEFORE there is a gglib binary to ask:
# the Rust and Node toolchains and the C/C++ build tools that `make setup`
# compiles with, and, under WSL2, the kernel setting that crashes npm.
#
# Everything else a build or a run needs — a GPU runtime and what building
# for it takes, OpenSSL, Python, and on Linux the libraries the desktop app
# links — is `gglib config check-deps`'s list, the one the GUI shows too.
# When a gglib binary exists this script ends by running that command, and
# its exit status is the script's. When none does it says how to run it.

# Don't use set -e here because we want to check ALL dependencies before exiting

# ANSI color codes
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
BOLD='\033[1m'
RESET='\033[0m'

# Source Rust environment if it exists (needed for non-interactive shells like VS Code tasks)
if [ -f "$HOME/.cargo/env" ]; then
    source "$HOME/.cargo/env"
fi

# Track results
MISSING_REQUIRED=()
PRESENT_REQUIRED=()

# Helper function to check if command exists
command_exists() {
    command -v "$1" >/dev/null 2>&1
}

# Helper function to get command version
get_version() {
    local cmd=$1
    local version_output

    case "$cmd" in
        npm)
            version_output="v$($cmd --version 2>/dev/null)"
            ;;
        *)
            version_output=$($cmd --version 2>/dev/null | head -n1)
            ;;
    esac

    echo "$version_output" | awk '{for(i=1;i<=NF;i++) if($i ~ /^[0-9]/) {print $i; exit}}'
}

# Dedicated Node.js checker that validates version meets the project minimum:
# package.json engines: "^20.19.0 || ^22.12.0 || >=24.0.0"
check_node_version() {
    local description="Required for building web UI and Tauri (>=20.19, 22.12, or 24+)"
    local min_node_20_minor=19
    local min_node_22_minor=12

    if ! command_exists node; then
        printf "%-20s ${RED}%-2s %-12s${RESET} %-50s\n" "node" "✗" "MISSING" "$description"
        echo -e "   ${YELLOW}Hint: run 'mise install' or 'nvm use' in the repo root to activate the version from .nvmrc${RESET}"
        MISSING_REQUIRED+=("node")
        return 1
    fi

    local version
    version=$(node --version 2>/dev/null | sed 's/^v//')
    local major minor patch
    IFS='.' read -r major minor patch <<< "$version"
    major=${major:-0}
    minor=${minor:-0}

    local ok=false
    if [ "$major" -ge 24 ]; then
        ok=true
    elif [ "$major" -eq 22 ] && [ "$minor" -ge "$min_node_22_minor" ]; then
        ok=true
    elif [ "$major" -eq 20 ] && [ "$minor" -ge "$min_node_20_minor" ]; then
        ok=true
    fi

    if [ "$ok" = true ]; then
        printf "%-20s ${GREEN}%-2s %-12s${RESET} %-50s\n" "node" "✓" "v$version" "$description"
        PRESENT_REQUIRED+=("node")
        return 0
    else
        printf "%-20s ${RED}%-2s %-12s${RESET} %-50s\n" "node" "✗" "v$version (TOO OLD)" "$description"
        echo -e "   ${YELLOW}Node.js v$version is installed but v20.19+, v22.12+, or v24+ is required.${RESET}"
        echo -e "   ${YELLOW}Run: mise install  (or: nvm install 22 && nvm use 22)  — see CONTRIBUTING.md for setup${RESET}"
        MISSING_REQUIRED+=("node")
        return 1
    fi
}

# Check a single required tool
check_dep() {
    local name=$1
    local description=$2

    if command_exists "$name"; then
        local version=$(get_version "$name")
        printf "%-20s ${GREEN}%-2s %-12s${RESET} %-50s\n" "$name" "✓" "$version" "$description"
        PRESENT_REQUIRED+=("$name")
        return 0
    else
        printf "%-20s ${RED}%-2s %-12s${RESET} %-50s\n" "$name" "✗" "MISSING" "$description"
        MISSING_REQUIRED+=("$name")
        return 1
    fi
}

# Detect OS and distribution
detect_os() {
    if [[ "$OSTYPE" == "darwin"* ]]; then
        echo "macos"
    elif [[ "$OSTYPE" == "linux-gnu"* ]] || [[ "$OSTYPE" == "linux" ]]; then
        echo "linux"
    elif [[ "$OSTYPE" == "msys" ]] || [[ "$OSTYPE" == "cygwin" ]] || [[ "$OSTYPE" == "win32" ]]; then
        echo "windows"
    else
        echo "unknown"
    fi
}

detect_linux_distro() {
    if [ -f /etc/os-release ]; then
        . /etc/os-release
        if [[ "$ID" == "ubuntu" ]] || [[ "$ID" == "debian" ]] || [[ "$ID_LIKE" == *"debian"* ]]; then
            echo "debian"
        elif [[ "$ID" == "fedora" ]] || [[ "$ID_LIKE" == *"fedora"* ]]; then
            echo "fedora"
        elif [[ "$ID" == "arch" ]] || [[ "$ID_LIKE" == *"arch"* ]]; then
            echo "arch"
        elif [[ "$ID" == "opensuse"* ]]; then
            echo "suse"
        else
            echo "linux-unknown"
        fi
    else
        echo "linux-unknown"
    fi
}

# Locate a gglib binary to hand over to (prefer local builds, then PATH).
find_gglib() {
    if [ -f "./target/release/gglib" ]; then
        echo "./target/release/gglib"
    elif [ -f "./target/debug/gglib" ]; then
        echo "./target/debug/gglib"
    elif command_exists gglib; then
        echo "gglib"
    else
        echo ""
    fi
}

# Print installation instructions for the tools above that are missing
print_install_instructions() {
    local os=$(detect_os)
    local distro=""
    if [ "$os" = "linux" ]; then
        distro=$(detect_linux_distro)
    fi

    echo ""
    echo -e "${BOLD}${BLUE}Installation Instructions:${RESET}"
    echo ""

    # Determine platform name
    local platform_name="Unknown"
    case "$os" in
        macos) platform_name="macOS" ;;
        windows) platform_name="Windows" ;;
        linux)
            case "$distro" in
                debian) platform_name="Ubuntu/Debian" ;;
                fedora) platform_name="Fedora" ;;
                arch) platform_name="Arch Linux" ;;
                suse) platform_name="openSUSE" ;;
                *) platform_name="Linux" ;;
            esac
            ;;
    esac

    echo -e "${BOLD}Platform detected: ${platform_name}${RESET}"
    echo ""

    # Check what's missing
    local need_rust=false
    local need_node=false
    local need_build_tools=false

    for dep in "${MISSING_REQUIRED[@]}"; do
        case "$dep" in
            cargo|rustc) need_rust=true ;;
            node|npm) need_node=true ;;
            git|make|gcc|g++|pkg-config|cmake) need_build_tools=true ;;
        esac
    done

    local step=1

    # 1. Rust installation
    if [ "$need_rust" = true ]; then
        echo -e "${BOLD}${step}. Install Rust toolchain:${RESET}"
        case "$os" in
            windows)
                echo -e "   ${YELLOW}Download and run:${RESET}"
                echo "   https://win.rustup.rs/x86_64"
                ;;
            *)
                echo -e "   ${YELLOW}curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh${RESET}"
                ;;
        esac
        echo ""
        ((step++))
    fi

    # 2. Node.js installation
    if [ "$need_node" = true ]; then
        echo -e "${BOLD}${step}. Install Node.js:${RESET}"
        case "$os" in
            macos)
                echo -e "   ${YELLOW}# Using Homebrew:${RESET}"
                echo "   brew install node"
                ;;
            windows)
                echo -e "   ${YELLOW}Download installer from:${RESET}"
                echo "   https://nodejs.org"
                ;;
            linux)
                case "$distro" in
                    debian)
                        echo -e "   ${YELLOW}# Ubuntu/Debian:${RESET}"
                        echo "   curl -fsSL https://deb.nodesource.com/setup_lts.x | sudo -E bash -"
                        echo "   sudo apt install -y nodejs"
                        ;;
                    fedora)
                        echo -e "   ${YELLOW}# Fedora:${RESET}"
                        echo "   sudo dnf install -y nodejs npm"
                        ;;
                    arch)
                        echo -e "   ${YELLOW}# Arch Linux:${RESET}"
                        echo "   sudo pacman -S nodejs npm"
                        ;;
                    *)
                        echo -e "   ${YELLOW}Visit: https://nodejs.org${RESET}"
                        ;;
                esac
                ;;
        esac
        echo ""
        ((step++))
    fi

    # 3. Build tools
    if [ "$need_build_tools" = true ]; then
        echo -e "${BOLD}${step}. Install build tools:${RESET}"
        case "$os" in
            macos)
                echo -e "   ${YELLOW}# Install Xcode Command Line Tools:${RESET}"
                echo "   xcode-select --install"
                echo ""
                echo -e "   ${YELLOW}# Using Homebrew (if needed):${RESET}"
                echo "   brew install pkg-config cmake"
                ;;
            windows)
                echo -e "   ${YELLOW}# Install Visual Studio Build Tools:${RESET}"
                echo "   https://visualstudio.microsoft.com/downloads/"
                echo "   (Select 'Desktop development with C++')"
                echo ""
                echo -e "   ${YELLOW}# Install Git:${RESET}"
                echo "   https://git-scm.com/download/win"
                ;;
            linux)
                case "$distro" in
                    debian)
                        echo -e "   ${YELLOW}# Ubuntu/Debian:${RESET}"
                        echo "   sudo apt update && sudo apt install -y build-essential git pkg-config cmake"
                        ;;
                    fedora)
                        echo -e "   ${YELLOW}# Fedora:${RESET}"
                        echo "   sudo dnf groupinstall -y 'Development Tools'"
                        echo "   sudo dnf install -y git pkg-config cmake"
                        ;;
                    arch)
                        echo -e "   ${YELLOW}# Arch Linux:${RESET}"
                        echo "   sudo pacman -S base-devel git pkg-config cmake"
                        ;;
                    *)
                        echo -e "   ${YELLOW}Install: git, make, gcc, g++, pkg-config, cmake${RESET}"
                        ;;
                esac
                ;;
        esac
        echo ""
        ((step++))
    fi
}

# WSL2 only: vm.mmap_rnd_bits=32 (the WSL2 default) causes Node.js/V8 to crash
# during npm install with "Fatal JavaScript invalid size error"
# (crbug.com/1201626). V8 pointer compression requires a 4 GB-aligned 4 GB
# contiguous VA window; with rnd_bits=32 the allocator can rarely find one.
# 28 bits is the fix. npm runs before any gglib binary is built, so this is
# checked here and nowhere else.
check_wsl2_mmap_rnd_bits() {
    grep -qi microsoft /proc/version 2>/dev/null || return 0

    # /proc/sys/vm/mmap_rnd_bits may be permission-denied for non-root on some
    # WSL2 kernels, so try sudo sysctl as a fallback.
    local mmap_rnd_bits
    mmap_rnd_bits=$(sysctl -n vm.mmap_rnd_bits 2>/dev/null \
        || cat /proc/sys/vm/mmap_rnd_bits 2>/dev/null \
        || sudo sysctl -n vm.mmap_rnd_bits 2>/dev/null \
        || echo "unknown")
    if [ "$mmap_rnd_bits" = "unknown" ]; then
        # Can't read even with sudo — emit a warning but don't block the build.
        # The user may have already applied the fix or be running a kernel that
        # doesn't expose this knob.
        printf "%-20s ${YELLOW}%-2s %-12s${RESET} %-50s\n" "vm.mmap_rnd_bits" "?" "unknown" "WSL2: unreadable — if npm crashes, run: sudo sysctl -w vm.mmap_rnd_bits=28"
    elif [ "$mmap_rnd_bits" -gt 28 ] 2>/dev/null; then
        printf "%-20s ${RED}%-2s %-12s${RESET} %-50s\n" "vm.mmap_rnd_bits" "✗" "$mmap_rnd_bits" "WSL2: must be ≤28 or npm/Node.js will crash (V8 crbug.com/1201626)"
        MISSING_REQUIRED+=("vm.mmap_rnd_bits")
        echo ""
        echo -e "  ${RED}▶ Fix Node.js/V8 crash on WSL2:${RESET}"
        echo -e "    ${BOLD}sudo sysctl -w vm.mmap_rnd_bits=28${RESET}   ${YELLOW}# immediate (current session)${RESET}"
        echo -e "    ${BOLD}echo 'vm.mmap_rnd_bits=28' | sudo tee /etc/sysctl.d/99-wsl-node.conf${RESET}   ${YELLOW}# persistent${RESET}"
        echo ""
    else
        printf "%-20s ${GREEN}%-2s %-12s${RESET} %-50s\n" "vm.mmap_rnd_bits" "✓" "$mmap_rnd_bits" "WSL2: Node.js/V8 VA layout safe"
    fi
}

# Main execution
main() {
    echo -e "${BOLD}${BLUE}Checking what building gglib needs...${RESET}"
    echo ""

    # Print header
    printf "${BOLD}%-20s %-15s %-50s${RESET}\n" "DEPENDENCY" "STATUS" "NOTES"
    echo "====================================================================================="

    check_dep "cargo" "Required for building Rust code"
    check_dep "rustc" "Rust compiler"
    check_node_version
    check_dep "npm" "Node package manager"
    check_dep "git" "Required for llama.cpp installation"
    check_dep "make" "Required for llama.cpp build"
    check_dep "gcc" "Required for llama.cpp compilation"
    check_dep "g++" "Required for llama.cpp compilation"
    check_dep "pkg-config" "Required for building with system libraries"
    check_dep "cmake" "Required for llama.cpp build"
    check_wsl2_mmap_rnd_bits

    echo ""
    echo "====================================================================================="

    local total_required=$((${#PRESENT_REQUIRED[@]} + ${#MISSING_REQUIRED[@]}))

    if [ ${#MISSING_REQUIRED[@]} -ne 0 ]; then
        echo -e "${RED}✗ ${#MISSING_REQUIRED[@]} required dependencies are missing.${RESET} (${#PRESENT_REQUIRED[@]}/$total_required)"
        print_install_instructions
        echo -e "${BOLD}After installing dependencies, run: ${BLUE}make setup${RESET}"
        return 1
    fi

    echo -e "${GREEN}✓ The tools that build gglib are installed.${RESET} (${#PRESENT_REQUIRED[@]}/$total_required)"
    echo ""

    # The rest of the list is gglib's own. Hand over to it when there is a
    # binary to run; its report and its exit status become this script's.
    local gglib_bin
    gglib_bin=$(find_gglib)
    if [ -n "$gglib_bin" ]; then
        echo "The rest is checked by: $gglib_bin config check-deps"
        echo ""
        exec "$gglib_bin" config check-deps
    fi

    echo "A GPU runtime, and on Linux the libraries the desktop app links, are"
    echo "checked by gglib itself once it is built:"
    echo "  cargo run -p gglib-cli -- config check-deps"
    echo ""
    echo -e "${BOLD}You can now run: ${BLUE}make setup${RESET}"
    return 0
}

# Run main function
main
