# capabilities

<!-- module-docs:start -->

Model capability detection.

This module analyzes GGUF metadata to detect model capabilities
such as reasoning/thinking support and tool/function calling; the tool
guess is core's `infer_from_chat_template`.

# Structure

- `reasoning` - Reasoning/thinking model detection
- `mtp` - Multi-Token Prediction draft head detection
- `embedding` - Embedding-model detection, from pooling type or an encoder-only architecture
- `patterns` - The reasoning detector's pattern constants

<!-- module-docs:end -->
