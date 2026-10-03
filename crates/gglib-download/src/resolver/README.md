# resolver

<!-- module-docs:start -->

`HuggingFace` file resolution.

This module resolves quantization-specific files from `HuggingFace` repositories
using the `HfClientPort` abstraction.

A resolution is a download group: the quantization's weights first, in shard
order, then the projector fetched with them when the repository has one. The
shard count and `is_sharded` describe the weights alone.

<!-- module-docs:end -->
