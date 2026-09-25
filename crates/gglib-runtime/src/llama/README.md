# llama

<!-- module-docs:start -->

Llama.cpp management for gglib-runtime.

This module provides all llama.cpp-related functionality:
- Installation (pre-built download or source build)
- Hardware acceleration detection (Metal, CUDA, CPU)
- Binary validation and status checking
- Update management

# Public API

The public API is intentionally minimal. Import from `gglib_runtime::llama`:

```rust,ignore
use gglib_runtime::llama::{
    ensure_llama_initialized,
    check_llama_installed,
    resolve_context_size,
};
```

# Feature Flags

- `cli`: Enables `CliPrompt` for interactive CLI usage.

<!-- module-docs:end -->
