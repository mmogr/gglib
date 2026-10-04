# huggingface

<!-- module-docs:start -->

`HuggingFace` client port definitions.

This module defines the port trait and DTOs for `HuggingFace` Hub interaction.
The actual implementation lives in `gglib-hf`.

A repository's quantizations are made of weights files alone, and its projectors
are listed apart (`list_projectors`). `download_group` answers what one download
fetches: a quantization's weights, and the projector chosen to go with them.
`projector_fetched_with` is that choice alone, for a listing that shows which
projector comes with each quantization.

<!-- module-docs:end -->
