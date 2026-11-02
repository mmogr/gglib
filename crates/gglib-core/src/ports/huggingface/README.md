# huggingface

<!-- module-docs:start -->

`HuggingFace` client port definitions.

This module defines the port trait and DTOs for `HuggingFace` Hub interaction.
The actual implementation lives in `gglib-hf`.

`HfRepoInfo` is a repository's summary, whether a search found it or it was
looked up by its ID; its architecture and trained context length are the ones
the Hub read from the repository's GGUF header. `HfSortField` is the one list
of orders a search can ask for: the browser sends it on the wire and the CLI's
`--sort` maps onto it.

A repository's variants (`list_quantizations`, `HfQuantInfo`) are its GGUF files
grouped into shard sets, each labelled as its uploader named it (`variants_of`),
and a request names one by its label (`match_label`). What a variant is comes
from a 64 KiB head of its first file (`variant_role`, `read_roles`): a draft or
helper is listed and never downloaded (`HELPER_FILES_REFUSAL`). The projectors are also listed apart (`list_projectors`).
`download_group` answers what one download fetches: a variant's files, and the
projector chosen to go with them. `projector_fetched_with` is that choice alone,
for a listing that shows which projector comes with each variant.

A search asks for one `HfModelKind`: models that chat (the default) or models
that draw. `read_head` reads a file's first bytes (at most `SNIFF_HEAD_BYTES`
are asked when a download wants to know what a weights file is before fetching
it), and `file_at` looks one file up by its path, for a file that is no
quantization, such as an image model's VAE. `file_url` is the address a
download fetches a file's bytes from: the Hub's own (`hub_file_url`) unless a
client says otherwise.

`image_companions` is what an image model's download brings beside its weights:
the head of the first weights file names the family, and each companion of the
family's recipe is looked up in its own repository. A head that cannot be read,
or that is a chat model's, is no image model and fails nothing.

<!-- module-docs:end -->
