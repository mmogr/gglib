# resolver

<!-- module-docs:start -->

HuggingFace file resolution.

Resolves quantization-specific files from HuggingFace repositories using the `HfClientPort` abstraction.

## Key Type: `HfQuantizationResolver`

Implements `QuantizationResolver` trait to:

1. Query HuggingFace for files matching a quantization (e.g., `Q4_K_M`)
2. Detect sharded models (multiple parts like `-00001-of-00004`)
3. Return `Resolution` with download URLs and metadata

## Flow

```text
User selects "Q4_K_M"   HfQuantizationResolver    HuggingFace API
        │                       │                       │
        └─── resolve(Q4_K_M) ──▶│                       │
                                └─── get_quant_files ──▶│
                                ◀── [file1, file2...] ──┘
        ◀─── Resolution {...} ─┘
```

<!-- module-docs:end -->

<details>
<summary><h2>Modules</h2></summary>

<!-- module-table:start -->
| Module | LOC | Complexity | Coverage |
|--------|-----|------------|----------|
<!-- module-table:end -->

</details>
