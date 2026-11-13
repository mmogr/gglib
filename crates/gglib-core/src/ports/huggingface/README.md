# huggingface

<!-- module-docs:start -->

`HuggingFace` client port definitions.

This module defines the port trait and DTOs for `HuggingFace` Hub interaction.
The actual implementation lives in `gglib-hf`.

`HfRepoInfo` is a repository's summary, whether a search found it or it was
looked up by its ID; its architecture and trained context length are the ones
the Hub read from the repository's GGUF header, and `file_paths` the files the
answer named. The Hub reads that header from one file, which can be a draft's.
A model file's name usually states its size (`named_parameters_b`: `27B`, never
a date like `2511`); a search sizes a hit's files only when those names
contradict the Hub's count (`names_contradict`), and then takes the count the
largest weights (`largest_weights`) could hold, at 1 to 34 bits a weight
(`plausible_total`, `sized_parameters_b`), dropping the Hub's context with its
count. `HfSortField` is the one list
of orders a search can ask for: the browser sends it on the wire and the CLI's
`--sort` maps onto it.

A repository's variants (`list_quantizations`, `HfQuantInfo`) are its GGUF files
grouped into shard sets, each labelled as its uploader named it with a quant
spelt in capitals (`variants_of`: `Q6_k` is `Q6_K`), and a request names one by
its label in any case (`match_label`), or by its whole file name,
as a pasted file's URL asks (`match_variant`). What a variant is comes
from a 64 KiB head of its first file (`variant_role`, `read_roles`): a draft or
helper is listed and never downloaded (`HELPER_FILES_REFUSAL`). A model stored
without a label is found again by the variant that holds its file
(`stored_label`). `download_group` answers what one download fetches: a
variant's weights, and the repository's projector (`repository_projector`), the
largest file whose head says it is one, the same for every variant.
`largest_projector` is that choice alone, over variants whose roles are read.
`said_as_a_quantization` is the one rule that reads a name for what a variant
is: a projector's or an importance matrix's file is not said as a
quantisation in a search's summary line, nor judged as weights by size
(`largest_weights`, for a variant whose header is unread).

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
