# llama

<!-- module-docs:start -->

Llama.cpp management for gglib-runtime.

This module provides all llama.cpp-related functionality:
- Installation (pre-built download or source build)
- Hardware acceleration detection (Metal, CUDA, Vulkan)
- Binary validation and status checking
- Update management

It asks the user nothing. A command that needs a yes before it installs or
updates asks for it in `gglib-cli`, and draws the progress there too.

# Public API

The public API is intentionally minimal. Import from `gglib_runtime::llama`:

```rust,ignore
use gglib_runtime::llama::{
    check_llama_installed,
    llama_status,
    update_preflight,
};
```

<!-- module-docs:end -->
