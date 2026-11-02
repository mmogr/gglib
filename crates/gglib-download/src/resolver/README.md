# resolver

<!-- module-docs:start -->

`HuggingFace` file resolution.

This module resolves quantization-specific files from `HuggingFace` repositories
using the `HfClientPort` abstraction.

A resolution is a download group: the variant's weights first, in shard
order, then the projector fetched with them when the repository has one (the
largest file whose head says it is one), then an image model's companions,
each from its own repository. The shard count and `is_sharded` describe the
weights alone.

An image model is known by the head of its first weights file, read before
anything is fetched (`image_companions`). The resolver of an upgrade, and of
the forced re-download of `update_model` (`own_files`), reads heads for the
projector and brings no companion: those are in their own folders and linked
already.

<!-- module-docs:end -->
