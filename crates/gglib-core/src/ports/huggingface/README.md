# huggingface

<!-- module-docs:start -->

`HuggingFace` client port definitions.

This module defines the port trait and DTOs for `HuggingFace` Hub interaction.
The actual implementation lives in `gglib-hf`.

`HfRepoInfo` is a repository's summary, whether a search found it or it was
looked up by its ID, and `HfSortField` is the one list of orders a search can
ask for: the browser sends it on the wire and the CLI's `--sort` maps onto it.

A repository's quantizations are made of weights files alone, and its projectors
are listed apart (`list_projectors`). `download_group` answers what one download
fetches: a quantization's weights, and the projector chosen to go with them.
`projector_fetched_with` is that choice alone, for a listing that shows which
projector comes with each quantization.

A search asks for one `HfModelKind`: models that chat (the default) or models
that draw. `read_head` reads a file's first bytes (at most `SNIFF_HEAD_BYTES`
are asked when a download wants to know what a weights file is before fetching
it), and `file_at` looks one file up by its path, for a file that is no
quantization, such as an image model's VAE.

`image_companions` is what an image model's download brings beside its weights:
the head of the first weights file names the family, and each companion of the
family's recipe is looked up in its own repository. A head that cannot be read,
or that is a chat model's, is no image model and fails nothing.

<!-- module-docs:end -->
