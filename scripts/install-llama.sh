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
        echo "🎮 Vulkan detected: Installing with Vulkan support" >&2
        echo "--vulkan"
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
