# sys

<!-- module-docs:start -->

The crate's only unsafe code: the C shim in `csrc/`, over llama.h at the
pin, and the handles that own what it hands out. Compiled only when build.rs
compiled the shim and linked llama.framework (`cfg(gglib_llama)`); every
unsafe item carries its own `#[expect(unsafe_code, reason = "...")]`, since
the workspace denies `unsafe_code` and a crate may not loosen its lints.

[`Model`], [`Context`] and [`Sampler`] each own one object the shim made and
free it when dropped. A context and a sampler hold their model in an `Arc`,
so it outlives them, and a context frees itself before the abort flag it was
watching. llama.cpp calls back into Rust twice: a load reports its progress
to `on_progress`, which answers whether to go on (a panic there is carried
past the C frames and resumed once the load returns), and a decode asks
`abort_requested` whether to stop, reading the [`Abort`] flag the context
holds a clone of. `Backend` keeps llama's backends up while any decoder
exists. A call that fails is answered with what llama.cpp logged as errors
during it, which the shim keeps per thread.

[`Abort`]: crate::Abort

# Structure

- `mod.rs` - the shim's functions as `csrc/gglib_llama.h` declares them, the
  errors it kept, and `Backend`
- `model.rs` - [`Model`]: a load with its progress and abort, the vocabulary
  and template, tokenizing, and a token's piece
- `context.rs` - [`Context`]: a decode, the abort it asks, and the KV cache
  cut back to a prefix
- `sampler.rs` - [`Sampler`]: llama-server's chain for one generation

<!-- module-docs:end -->
