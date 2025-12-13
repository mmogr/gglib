#!/bin/bash
set -e

# Function to detect GPU and return flags
detect_gpu_flags() {
    if [ "$(uname)" = "Darwin" ]; then
        echo "🍎 macOS detected: Installing with Metal support" >&2
        echo "--metal"
    elif command -v nvcc >/dev/null 2>&1; then
        CUDA_VERSION=$(nvcc --version | grep "release" | sed -n 's/.*release \([0-9.]*\).*/\1/p')
        echo "🚀 CUDA $CUDA_VERSION detected: Installing with CUDA support" >&2
        echo "--cuda"
    else
        echo "💻 No GPU acceleration detected: Installing CPU-only version" >&2
        echo "--cpu-only"
    fi
}

# Main execution
echo "Detecting GPU configuration..."
GPU_FLAGS=$(detect_gpu_flags)

# Find gglib binary - prefer local builds to use repo paths
if [ -f "./target/release/gglib" ]; then
    GGLIB_BIN="./target/release/gglib"
elif [ -f "./target/debug/gglib" ]; then
    GGLIB_BIN="./target/debug/gglib"
elif command -v gglib >/dev/null 2>&1; then
    GGLIB_BIN="gglib"
else
    echo "Error: gglib binary not found. Please build it first."
    exit 1
fi

echo "Running: $GGLIB_BIN llama install $GPU_FLAGS"
$GGLIB_BIN llama install $GPU_FLAGS
