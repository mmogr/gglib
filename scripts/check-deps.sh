#!/usr/bin/env bash

# Bootstrap dependency checker for gglib
# This script checks system dependencies WITHOUT requiring Rust compilation
# It's designed to run BEFORE any cargo commands to catch missing build tools

# Don't use set -e here because we want to check ALL dependencies before exiting

# ANSI color codes
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
BOLD='\033[1m'
RESET='\033[0m'

# Track results
MISSING_REQUIRED=()
PRESENT_REQUIRED=()
MISSING_OPTIONAL=()

# Helper function to check if command exists
command_exists() {
    command -v "$1" >/dev/null 2>&1
}

# Helper function to get command version
get_version() {
    local cmd=$1
    local version_output
    
    case "$cmd" in
        cargo|rustc|git|cmake|make|gcc|g++|pkg-config|node)
            version_output=$($cmd --version 2>/dev/null | head -n1)
            ;;
        npm)
            version_output="v$($cmd --version 2>/dev/null)"
            ;;
        nvcc)
            version_output=$($cmd --version 2>/dev/null | grep "release" | sed -n 's/.*release \([0-9.]*\).*/\1/p')
            ;;
        *)
            version_output="installed"
            ;;
    esac
    
    echo "$version_output" | awk '{for(i=1;i<=NF;i++) if($i ~ /^[0-9]/) {print $i; exit}}'
}

# Dedicated Python 3 checker (accepts python3/python/py -3)
check_python() {
    local description="Required for hf_xet fast download helper"
    local cmd=""
    local version=""

    if command_exists python3; then
        cmd="python3"
        version=$(python3 --version 2>&1 | awk '{print $2}')
    elif command_exists python; then
        cmd="python"
        version=$(python --version 2>&1 | awk '{print $2}')
    elif command_exists py; then
        # Windows launcher – prefer Python 3 explicitly
        if py -3 --version >/dev/null 2>&1; then
            cmd="py -3"
            version=$(py -3 --version 2>&1 | awk '{print $2}')
        fi
    fi

    if [ -n "$cmd" ]; then
        local major=${version%%.*}
        if [ "$major" -ge 3 ] 2>/dev/null; then
            printf "%-20s ${GREEN}%-2s %-12s${RESET} %-50s\n" "python3" "✓" "$version" "$description"
            PRESENT_REQUIRED+=("python3")
            return 0
        fi
    fi

    printf "%-20s ${RED}%-2s %-12s${RESET} %-50s\n" "python3" "✗" "MISSING" "$description"
    MISSING_REQUIRED+=("python3")
    return 1
}

# Check a single dependency
check_dep() {
    local name=$1
    local description=$2
    local required=$3
    local check_cmd=$4
    
    if command_exists "$check_cmd"; then
        local version=$(get_version "$check_cmd")
        printf "%-20s ${GREEN}%-2s %-12s${RESET} %-50s\n" "$name" "✓" "$version" "$description"
        if [ "$required" = "true" ]; then
            PRESENT_REQUIRED+=("$name")
        fi
        return 0
    else
        if [ "$required" = "true" ]; then
            printf "%-20s ${RED}%-2s %-12s${RESET} %-50s\n" "$name" "✗" "MISSING" "$description"
            MISSING_REQUIRED+=("$name")
            return 1
        else
            printf "%-20s ${YELLOW}%-2s %-12s${RESET} %-50s\n" "$name" "⚠" "optional" "$description"
            MISSING_OPTIONAL+=("$name")
            return 0
        fi
    fi
}

# Check library with pkg-config
check_lib() {
    local name=$1
    local description=$2
    local required=$3
    local pkg_name=$4
    
    if command_exists pkg-config && pkg-config --exists "$pkg_name" 2>/dev/null; then
        local version=$(pkg-config --modversion "$pkg_name" 2>/dev/null)
        printf "%-20s ${GREEN}%-2s %-12s${RESET} %-50s\n" "$name" "✓" "$version" "$description"
        if [ "$required" = "true" ]; then
            PRESENT_REQUIRED+=("$name")
        fi
        return 0
    else
        if [ "$required" = "true" ]; then
            printf "%-20s ${RED}%-2s %-12s${RESET} %-50s\n" "$name" "✗" "MISSING" "$description"
            MISSING_REQUIRED+=("$name")
            return 1
        else
            printf "%-20s ${YELLOW}%-2s %-12s${RESET} %-50s\n" "$name" "⚠" "optional" "$description"
            MISSING_OPTIONAL+=("$name")
            return 0
        fi
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

# Print installation instructions
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
    local need_gtk=false
    local need_cuda=false
    local cuda_not_in_path=false
    
    # Check if CUDA is installed but not in PATH (Linux/Windows only)
    local os=$(detect_os)
    if [ "$os" = "linux" ]; then
        if [ -d "/opt/cuda" ] || [ -d "/usr/local/cuda" ]; then
            if ! command_exists nvcc; then
                cuda_not_in_path=true
            fi
        fi
    elif [ "$os" = "windows" ]; then
        if [ -d "/c/Program Files/NVIDIA GPU Computing Toolkit/CUDA" ] || [ -d "/mnt/c/Program Files/NVIDIA GPU Computing Toolkit/CUDA" ]; then
            if ! command_exists nvcc; then
                cuda_not_in_path=true
            fi
        fi
    fi
    
    for dep in "${MISSING_REQUIRED[@]}"; do
        case "$dep" in
            cargo|rustc) need_rust=true ;;
            node|npm) need_node=true ;;
            git|make|gcc|g++|pkg-config|libssl-dev|cmake) need_build_tools=true ;;
            patchelf|webkit2gtk-4.1|librsvg|libappindicator-gtk3) need_gtk=true ;;
            CUDA|GPU) need_cuda=true ;;
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
                echo "   brew install pkg-config openssl cmake"
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
                        echo "   sudo apt update && sudo apt install -y \\"
                        echo "     build-essential git pkg-config libssl-dev libcurl4-openssl-dev cmake"
                        ;;
                    fedora)
                        echo -e "   ${YELLOW}# Fedora:${RESET}"
                        echo "   sudo dnf groupinstall -y 'Development Tools'"
                        echo "   sudo dnf install -y git pkg-config openssl-devel cmake"
                        ;;
                    arch)
                        echo -e "   ${YELLOW}# Arch Linux:${RESET}"
                        echo "   sudo pacman -S base-devel git pkg-config openssl cmake"
                        ;;
                    *)
                        echo -e "   ${YELLOW}Install: git, make, gcc, g++, pkg-config, openssl-dev, cmake${RESET}"
                        ;;
                esac
                ;;
        esac
        echo ""
        ((step++))
    fi
    
    # 4. GTK/Tauri dependencies (Linux only)
    if [ "$need_gtk" = true ]; then
        echo -e "${BOLD}${step}. Install GTK/Tauri dependencies:${RESET}"
        case "$distro" in
            debian)
                echo -e "   ${YELLOW}# Ubuntu/Debian:${RESET}"
                echo -e "   ${BLUE}# See: https://v2.tauri.app/start/prerequisites/${RESET}"
                echo "   sudo apt update && sudo apt install -y \\"
                echo "     libwebkit2gtk-4.1-dev \\"
                echo "     librsvg2-dev \\"
                echo "     libgtk-3-dev \\"
                echo "     libayatana-appindicator3-dev \\"
                echo "     patchelf"
                ;;
            fedora)
                echo -e "   ${YELLOW}# Fedora:${RESET}"
                echo "   sudo dnf install -y \\"
                echo "     webkit2gtk4.1-devel \\"
                echo "     librsvg2-devel \\"
                echo "     gtk3-devel \\"
                echo "     libappindicator-gtk3-devel \\"
                echo "     patchelf"
                ;;
            arch)
                echo -e "   ${YELLOW}# Arch Linux:${RESET}"
                echo "   sudo pacman -S \\"
                echo "     webkit2gtk-4.1 \\"
                echo "     librsvg \\"
                echo "     gtk3 \\"
                echo "     libappindicator-gtk3 \\"
                echo "     patchelf"
                ;;
            *)
                echo -e "   ${YELLOW}Install WebKit2GTK and librsvg development packages${RESET}"
                echo -e "   ${BLUE}See: https://v2.tauri.app/start/prerequisites/${RESET}"
                ;;
        esac
        echo ""
        ((step++))
    fi
    
    # 5. CUDA Toolkit (GPU acceleration)
    if [ "$need_cuda" = true ]; then
        if [ "$cuda_not_in_path" = true ]; then
            echo -e "${BOLD}${step}. Configure CUDA PATH:${RESET}"
            echo -e "   ${YELLOW}# CUDA is installed but nvcc not in PATH${RESET}"
            
            if [ "$os" = "linux" ]; then
                echo -e "   ${YELLOW}# Add to ~/.bashrc or ~/.zshrc:${RESET}"
                if [ -d "/opt/cuda" ]; then
                    echo "   export PATH=\"/opt/cuda/bin:\$PATH\""
                    echo "   export LD_LIBRARY_PATH=\"/opt/cuda/lib64:\$LD_LIBRARY_PATH\""
                elif [ -d "/usr/local/cuda" ]; then
                    echo "   export PATH=\"/usr/local/cuda/bin:\$PATH\""
                    echo "   export LD_LIBRARY_PATH=\"/usr/local/cuda/lib64:\$LD_LIBRARY_PATH\""
                fi
                echo ""
                echo -e "   ${YELLOW}# Then reload your shell:${RESET}"
                echo "   source ~/.bashrc"
            elif [ "$os" = "windows" ]; then
                echo -e "   ${YELLOW}# Add CUDA to PATH in System Environment Variables${RESET}"
                echo -e "   ${YELLOW}# Or run in PowerShell/CMD:${RESET}"
                echo "   set PATH=%PATH%;C:\\Program Files\\NVIDIA GPU Computing Toolkit\\CUDA\\vXX.X\\bin"
            fi
        else
            echo -e "${BOLD}${step}. Install CUDA Toolkit:${RESET}"
            case "$os" in
                macos)
                    echo -e "   ${RED}Metal is required on macOS (should be built-in)${RESET}"
                    ;;
                linux)
                    case "$distro" in
                        arch)
                            echo -e "   ${YELLOW}# Arch Linux:${RESET}"
                            echo "   yay -S cuda"
                            echo ""
                            echo -e "   ${YELLOW}# Then add to ~/.bashrc:${RESET}"
                            echo "   export PATH=\"/opt/cuda/bin:\$PATH\""
                            echo "   export LD_LIBRARY_PATH=\"/opt/cuda/lib64:\$LD_LIBRARY_PATH\""
                            ;;
                        debian)
                            echo -e "   ${YELLOW}# Ubuntu/Debian:${RESET}"
                            echo -e "   ${BLUE}# See: https://developer.nvidia.com/cuda-downloads${RESET}"
                            echo "   wget https://developer.download.nvidia.com/compute/cuda/repos/..."
                            ;;
                        *)
                            echo -e "   ${YELLOW}Download from: https://developer.nvidia.com/cuda-downloads${RESET}"
                            ;;
                    esac
                    ;;
                windows)
                    echo -e "   ${YELLOW}Download from: https://developer.nvidia.com/cuda-downloads${RESET}"
                    ;;
            esac
        fi
        echo ""
        ((step++))
    fi
    
    # Quick install summary
    echo -e "${BOLD}Quick Install (copy-paste):${RESET}"
    
    # Special handling for CUDA PATH issue (Linux only)
    if [ "$cuda_not_in_path" = true ] && [ "$os" = "linux" ]; then
        echo ""
        echo -e "  ${YELLOW}# CUDA installed but not in PATH - add to ~/.bashrc:${RESET}"
        if [ -d "/opt/cuda" ]; then
            echo -e "  ${YELLOW}echo 'export PATH=\"/opt/cuda/bin:\$PATH\"' >> ~/.bashrc${RESET}"
            echo -e "  ${YELLOW}echo 'export LD_LIBRARY_PATH=\"/opt/cuda/lib64:\$LD_LIBRARY_PATH\"' >> ~/.bashrc${RESET}"
        elif [ -d "/usr/local/cuda" ]; then
            echo -e "  ${YELLOW}echo 'export PATH=\"/usr/local/cuda/bin:\$PATH\"' >> ~/.bashrc${RESET}"
            echo -e "  ${YELLOW}echo 'export LD_LIBRARY_PATH=\"/usr/local/cuda/lib64:\$LD_LIBRARY_PATH\"' >> ~/.bashrc${RESET}"
        fi
        echo -e "  ${YELLOW}source ~/.bashrc${RESET}"
        echo ""
    fi
    
    case "$os" in
        macos)
            if [ "$need_rust" = true ]; then
                echo -e "  ${YELLOW}curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh${RESET}"
            fi
            if [ "$need_node" = true ] || [ "$need_build_tools" = true ]; then
                local pkgs=()
                [ "$need_node" = true ] && pkgs+=("node")
                [ "$need_build_tools" = true ] && pkgs+=("pkg-config" "openssl" "cmake")
                echo -e "  ${YELLOW}brew install ${pkgs[*]}${RESET}"
            fi
            ;;
        windows)
            echo -e "  ${YELLOW}1. Install Rust: https://win.rustup.rs/x86_64${RESET}"
            echo -e "  ${YELLOW}2. Install Node.js: https://nodejs.org${RESET}"
            echo -e "  ${YELLOW}3. Install VS Build Tools: https://visualstudio.microsoft.com/downloads/${RESET}"
            ;;
        linux)
            case "$distro" in
                debian)
                    if [ "$need_rust" = true ]; then
                        echo -e "  ${YELLOW}curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh${RESET}"
                    fi
                    if [ "$need_node" = true ] || [ "$need_build_tools" = true ] || [ "$need_gtk" = true ]; then
                        local cmd="sudo apt update && sudo apt install -y"
                        [ "$need_node" = true ] && cmd="$cmd nodejs npm"
                        [ "$need_build_tools" = true ] && cmd="$cmd build-essential git pkg-config libssl-dev libcurl4-openssl-dev patchelf cmake"
                        [ "$need_gtk" = true ] && cmd="$cmd libwebkit2gtk-4.1-dev librsvg2-dev libgtk-3-dev libayatana-appindicator3-dev"
                        echo -e "  ${YELLOW}${cmd}${RESET}"
                    fi
                    ;;
                fedora)
                    if [ "$need_rust" = true ]; then
                        echo -e "  ${YELLOW}curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh${RESET}"
                    fi
                    [ "$need_build_tools" = true ] && echo -e "  ${YELLOW}sudo dnf groupinstall -y 'Development Tools' && sudo dnf install -y git pkg-config openssl-devel patchelf cmake${RESET}"
                    [ "$need_node" = true ] && echo -e "  ${YELLOW}sudo dnf install -y nodejs npm${RESET}"
                    [ "$need_gtk" = true ] && echo -e "  ${YELLOW}sudo dnf install -y webkit2gtk4.1-devel librsvg2-devel gtk3-devel libappindicator-gtk3-devel${RESET}"
                    ;;
                arch)
                    if [ "$need_rust" = true ]; then
                        echo -e "  ${YELLOW}curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh${RESET}"
                    fi
                    if [ "$need_node" = true ] || [ "$need_build_tools" = true ] || [ "$need_gtk" = true ]; then
                        local cmd="sudo pacman -S"
                        [ "$need_node" = true ] && cmd="$cmd nodejs npm"
                        [ "$need_build_tools" = true ] && cmd="$cmd base-devel git pkg-config openssl patchelf cmake"
                        [ "$need_gtk" = true ] && cmd="$cmd webkit2gtk-4.1 librsvg gtk3 libappindicator-gtk3"
                        echo -e "  ${YELLOW}${cmd}${RESET}"
                    fi
                    ;;
                *)
                    echo -e "  ${YELLOW}See platform-specific instructions above${RESET}"
                    ;;
            esac
            ;;
    esac
    echo ""
}

# Main execution
main() {
    echo -e "${BOLD}${BLUE}Checking system dependencies...${RESET}"
    echo ""
    
    # Print header
    printf "${BOLD}%-20s %-15s %-50s${RESET}\n" "DEPENDENCY" "STATUS" "NOTES"
    echo "====================================================================================="
    
    # Check core dependencies
    check_dep "cargo" "Required for building Rust code" "true" "cargo"
    check_dep "rustc" "Rust compiler" "true" "rustc"
    check_dep "node" "Required for building web UI and Tauri" "true" "node"
    check_dep "npm" "Node package manager" "true" "npm"
    check_dep "git" "Required for llama.cpp installation" "true" "git"
    check_dep "make" "Required for llama.cpp build" "true" "make"
    check_dep "gcc" "Required for llama.cpp compilation" "true" "gcc"
    check_dep "g++" "Required for llama.cpp compilation" "true" "g++"
    check_dep "pkg-config" "Required for building with system libraries" "true" "pkg-config"
    check_lib "libssl-dev" "Required for HTTPS support" "true" "openssl"
    check_dep "cmake" "Required for llama.cpp build" "true" "cmake"
    check_python
    
    # Check Linux-specific dependencies
    if [[ "$OSTYPE" == "linux-gnu"* ]] || [[ "$OSTYPE" == "linux" ]]; then
        # Check curl development headers (required for llama.cpp on Linux)
        check_lib "libcurl-dev" "Required for llama.cpp HTTP/HTTPS support" "true" "libcurl"
        
        check_dep "patchelf" "Required for Tauri AppImage bundling (linuxdeploy)" "true" "patchelf"

        # On some rolling distros (notably Arch), linuxdeploy's bundled binutils can be too old
        # to strip modern system libraries that use RELR relocations, causing AppImage bundling to fail.
        # Workaround: build with NO_STRIP=1 (Makefile sets this automatically for Linux builds).
        if [ "$(detect_linux_distro)" = "arch" ] && [ -z "${NO_STRIP:-}" ]; then
            echo -e "${YELLOW}NOTE:${RESET} If AppImage bundling fails with 'unknown type [0x13] section .relr.dyn', run builds with ${BOLD}NO_STRIP=1${RESET}."
        fi
        # Try webkit2gtk-4.1 first (Ubuntu 24.04+), fallback to 4.0
        if ! pkg-config --exists webkit2gtk-4.1 2>/dev/null && pkg-config --exists webkit2gtk-4.0 2>/dev/null; then
            check_lib "webkit2gtk-4.1" "Required for Tauri desktop app (WebView)" "true" "webkit2gtk-4.0"
        else
            check_lib "webkit2gtk-4.1" "Required for Tauri desktop app (WebView)" "true" "webkit2gtk-4.1"
        fi
        check_lib "librsvg" "Required for Tauri desktop app (SVG rendering)" "true" "librsvg-2.0"
        # Try appindicator3-0.1 first (Arch), then ayatana variant (Ubuntu/Debian)
        if pkg-config --exists appindicator3-0.1 2>/dev/null; then
            check_lib "libappindicator-gtk3" "Required for Tauri system tray support" "true" "appindicator3-0.1"
        else
            check_lib "libappindicator-gtk3" "Required for Tauri system tray support" "true" "ayatana-appindicator3-0.1"
        fi
    fi
    
    # GPU acceleration check (required - CPU-only not supported)
    local os=$(detect_os)
    if [ "$os" = "macos" ]; then
        printf "%-20s ${GREEN}%-2s %-12s${RESET} %-50s\n" "Metal" "✓" "available" "Apple GPU acceleration (required)"
    elif command_exists nvcc; then
        local cuda_version=$(get_version nvcc)
        printf "%-20s ${GREEN}%-2s %-12s${RESET} %-50s\n" "CUDA Toolkit" "✓" "$cuda_version" "NVIDIA GPU acceleration (required)"
        PRESENT_REQUIRED+=("CUDA")
    elif command_exists nvidia-smi || (command_exists lspci && lspci 2>/dev/null | grep -i nvidia >/dev/null); then
        # GPU detected but nvcc not in PATH - check if CUDA is installed but not configured (platform-specific)
        local cuda_dir_exists=false
        if [ "$os" = "linux" ]; then
            [ -d "/opt/cuda" ] || [ -d "/usr/local/cuda" ] && cuda_dir_exists=true
        elif [ "$os" = "windows" ]; then
            [ -d "/c/Program Files/NVIDIA GPU Computing Toolkit/CUDA" ] || [ -d "/mnt/c/Program Files/NVIDIA GPU Computing Toolkit/CUDA" ] && cuda_dir_exists=true
        fi
        
        if [ "$cuda_dir_exists" = true ]; then
            printf "%-20s ${RED}%-2s %-12s${RESET} %-50s\n" "CUDA Toolkit" "✗" "NOT IN PATH" "CUDA installed but nvcc not in PATH - check installation"
        else
            printf "%-20s ${RED}%-2s %-12s${RESET} %-50s\n" "CUDA Toolkit" "✗" "NOT INSTALLED" "NVIDIA GPU detected but CUDA toolkit not installed"
        fi
        MISSING_REQUIRED+=("CUDA")
    elif [ "$os" = "linux" ] || [ "$os" = "windows" ]; then
        printf "%-20s ${RED}%-2s %-12s${RESET} %-50s\n" "GPU" "✗" "MISSING" "No GPU detected - CUDA (Linux/Windows) or Metal (macOS) required"
        MISSING_REQUIRED+=("GPU")
    fi
    
    echo ""
    echo "====================================================================================="
    
    # Summary
    local total_required=$((${#PRESENT_REQUIRED[@]} + ${#MISSING_REQUIRED[@]}))
    
    if [ ${#MISSING_REQUIRED[@]} -eq 0 ]; then
        echo -e "${GREEN}✓ All required dependencies are installed!${RESET} (${#PRESENT_REQUIRED[@]}/$total_required)"
        echo ""
        echo -e "${BOLD}You can now run: ${BLUE}make setup${RESET}"
        return 0
    else
        echo -e "${RED}✗ ${#MISSING_REQUIRED[@]} required dependencies are missing.${RESET} (${#PRESENT_REQUIRED[@]}/$total_required)"
        print_install_instructions
        echo -e "${BOLD}After installing dependencies, run: ${BLUE}make setup${RESET}"
        return 1
    fi
}

# Run main function
main
