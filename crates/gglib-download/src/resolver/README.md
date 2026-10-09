# resolver

<!-- module-docs:start -->

`HuggingFace` file resolution.

This module resolves quantization-specific files from `HuggingFace` repositories
using the `HfClientPort` abstraction.

A resolution is a download group: the quantization's weights first, in shard
order, then the projector fetched with them when the repository has one, then
an image model's companions, each from its own repository. The shard count and
`is_sharded` describe the weights alone.

An image model is known by the head of its first weights file, read before
anything is fetched (`image_companions`); a resolver made `weights_only`, as
the forced re-download of `update_model` is, reads no head and brings no
companion.

<!-- module-docs:end -->
