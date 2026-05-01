#!/bin/bash
set -e

# Locate the gglib binary (prefer local builds, then PATH).
find_gglib() {
    if [ -f "./target/release/gglib" ]; then
        echo "./target/release/gglib"
    elif [ -f "./target/debug/gglib" ]; then
        echo "./target/debug/gglib"
    elif command -v gglib >/dev/null 2>&1; then
        echo "gglib"
    else
        echo ""
    fi
}

# Detect GPU flags using the gglib binary when available,
# falling back to minimal inline detection otherwise.
detect_gpu_flags() {
    local bin
    bin=$(find_gglib)

    # Try the binary first — it has comprehensive detection including
    # Vulkan header/glslc validation that shell can't easily replicate.
    if [ -n "$bin" ]; then
        local json
        json=$("$bin" config llama detect --json 2>/dev/null) || true
        if [ -n "$json" ]; then
            local accel
            accel=$(echo "$json" | grep -oE '"acceleration"\s*:\s*"[^"]*"' | head -1 | cut -d'"' -f4)
            case "$accel" in
                Metal)
                    echo "🍎 macOS detected: Installing with Metal support" >&2
                    echo "--metal"
                    return
                    ;;
                CUDA)
                    echo "🚀 CUDA detected: Installing with CUDA support" >&2
                    echo "--cuda"
                    return
                    ;;
                Vulkan)
                    echo "🎮 Vulkan detected: Installing with Vulkan support" >&2
                    echo "--vulkan"
                    return
                    ;;
            esac

            # acceleration is null — strict-fail when a GPU runtime IS
            # detected but build deps are incomplete. The user almost
            # certainly wants to install the missing package and re-run,
            # not get a slow CPU build silently.
            local has_loader
            has_loader=$(echo "$json" | grep -oE '"hasLoader"\s*:\s*true' | head -1)
            if [ -n "$has_loader" ]; then
                echo "" >&2
                echo -e "\033[1;31m❌ Vulkan GPU detected but build dependencies are missing.\033[0m" >&2
                echo "" >&2
                echo "Run \`gglib config llama detect\` to see which packages to install," >&2
                echo "then re-run \`make setup\`. Refusing to silently downgrade to a CPU build." >&2
                exit 1
            fi
        fi
    fi

    # Inline fallback (pre-build — gglib binary may not exist yet).
    if [ "$(uname)" = "Darwin" ]; then
        echo "🍎 macOS detected: Installing with Metal support" >&2
        echo "--metal"
    elif command -v nvcc >/dev/null 2>&1; then
        CUDA_VERSION=$(nvcc --version | grep "release" | sed -n 's/.*release \([0-9.]*\).*/\1/p')
        echo "🚀 CUDA $CUDA_VERSION detected: Installing with CUDA support" >&2
        echo "--cuda"
    elif command -v vulkaninfo >/dev/null 2>&1 && vulkaninfo --summary >/dev/null 2>&1; then
        # Vulkan loader present — verify build deps inline before
        # promising a Vulkan build. If anything is missing, hard-fail
        # rather than silently degrading.
        local missing_inline=()
        command -v glslc >/dev/null 2>&1 || missing_inline+=("glslc")
        if ! { [ -f /usr/include/vulkan/vulkan.h ] \
            || [ -f /usr/local/include/vulkan/vulkan.h ] \
            || (command -v pkg-config >/dev/null 2>&1 && pkg-config --exists vulkan 2>/dev/null); }; then
            missing_inline+=("Vulkan headers (libvulkan-dev)")
        fi
        if ! { (command -v pkg-config >/dev/null 2>&1 && pkg-config --exists SPIRV-Headers 2>/dev/null) \
            || [ -f /usr/include/spirv/unified1/spirv.hpp ] \
            || [ -f /usr/local/include/spirv/unified1/spirv.hpp ] \
            || [ -f /usr/include/spirv-headers/spirv.hpp ] \
            || [ -f /usr/local/include/spirv-headers/spirv.hpp ] \
            || { [ -n "${VULKAN_SDK:-}" ] && [ -f "$VULKAN_SDK/Include/spirv/unified1/spirv.hpp" ]; }; }; then
            missing_inline+=("SPIR-V headers (spirv-headers)")
        fi

        if [ ${#missing_inline[@]} -eq 0 ]; then
            echo "🎮 Vulkan detected: Installing with Vulkan support" >&2
            echo "--vulkan"
        else
            echo "" >&2
            echo -e "\033[1;31m❌ Vulkan GPU detected but build dependencies are missing:\033[0m" >&2
            for m in "${missing_inline[@]}"; do
                echo "    - $m" >&2
            done
            echo "" >&2
            echo "Install the missing packages and re-run \`make setup\`." >&2
            echo "Refusing to silently downgrade to a CPU build." >&2
            exit 1
        fi
    else
        echo "💻 No GPU acceleration detected: Installing CPU-only version" >&2
        echo "--cpu-only"
    fi
}

# Main execution
echo "Detecting GPU configuration..."
GPU_FLAGS=$(detect_gpu_flags)

GGLIB_BIN=$(find_gglib)
if [ -z "$GGLIB_BIN" ]; then
    echo "Error: gglib binary not found. Please build it first."
    exit 1
fi

echo "Running: $GGLIB_BIN config llama install $GPU_FLAGS"
$GGLIB_BIN config llama install $GPU_FLAGS
