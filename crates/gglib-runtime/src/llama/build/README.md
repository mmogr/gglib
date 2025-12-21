# build

<!-- module-docs:start -->

Build orchestration for llama.cpp with progress tracking.

Handles compiling llama.cpp from source with appropriate acceleration flags.

## Acceleration Support

| Platform | Acceleration |
|----------|-------------|
| macOS | Metal (GPU) |
| Linux | CUDA or CPU |
| Windows | CUDA or CPU |

## Build Process

1. Configure with cmake (detect acceleration)
2. Build with parallel jobs
3. Progress bar shows compilation status
4. Validate built binaries

## Note

`CXXFLAGS="-O2"` is set to work around a GCC 15.2.1 segfault bug during optimization passes.

<!-- module-docs:end -->

<details>
<summary><h2>Modules</h2></summary>

<!-- module-table:start -->
| Module | LOC | Complexity | Coverage |
|--------|-----|------------|----------|
<!-- module-table:end -->

</details>
