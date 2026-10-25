# llama

<!-- module-docs:start -->

Llama.cpp management for gglib-runtime.

This module provides all llama.cpp-related functionality:
- Installation of the pinned release (pre-built download or source build)
- Hardware acceleration detection (Metal, CUDA, Vulkan)
- Binary validation and status checking

An update is an install of the pinned release: whether one has anything to
do, and whether it downloads or builds, are `crate::engine`'s to say, for
llama.cpp and stable-diffusion.cpp alike. This module asks the user
nothing. A command that needs a yes before it installs asks for it in
`gglib-cli`, and draws the progress there too.

# Public API

The public API is intentionally minimal. Import from `gglib_runtime::llama`:

```rust,ignore
use gglib_runtime::llama::{
    check_llama_installed,
    llama_status,
    download_prebuilt_binaries,
};
```

<!-- module-docs:end -->
