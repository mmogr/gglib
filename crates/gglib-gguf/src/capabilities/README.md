# capabilities

<!-- module-docs:start -->

Model capability detection.

This module analyzes GGUF metadata to detect model capabilities
such as reasoning/thinking support and tool/function calling.

# Structure

- `reasoning` - Reasoning/thinking model detection
- `tool_calling` - Tool/function calling detection
- `template_probe` - Tool-call dialect derivation by render-and-diff over
  the model's own chat template — produces the structured `DialectSpec`
  persisted per model, with the pattern tables as fallback
- `mtp` - Multi-Token Prediction draft head detection
- `embedding` - Embedding-model detection, from pooling type or an encoder-only architecture
- `patterns` - Pattern constants shared across detection modules

<!-- module-docs:end -->
