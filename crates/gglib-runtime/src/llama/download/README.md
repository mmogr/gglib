# download

<!-- module-docs:start -->

Pre-built llama.cpp binary download support.

Handles downloading pre-built llama.cpp binaries from GitHub releases for users running pre-built gglib binaries.

## Platform Support

| Platform | Status |
|----------|--------|
| macOS ARM64 | ✅ Metal-enabled pre-built |
| macOS x64 | ✅ Metal-enabled pre-built |
| Windows x64 | ✅ CUDA-enabled pre-built |
| Linux | ❌ Must build from source |

## Flow

1. Check GitHub releases for matching version
2. Download appropriate archive for platform
3. Extract binaries to data directory
4. Verify binaries are executable

<!-- module-docs:end -->

<details>
<summary><h2>Modules</h2></summary>

<!-- module-table:start -->
| Module | LOC | Complexity | Coverage |
|--------|-----|------------|----------|
<!-- module-table:end -->

</details>
