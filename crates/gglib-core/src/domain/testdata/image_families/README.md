# image_families

<!-- module-docs:start -->

The tensor tables of the eight image-model files measured on 2026-10-09, one
file per source. `image_family_tests.rs` reads them with `include_str!` to pin
`ImageFamily::sniff` and `ComponentRole::fits` against real files, and
`model_components_tests.rs` links them as components.

Each `.tsv` holds one line per tensor, sorted: the tensor's name, a tab, and
its shape outermost first as `AxBxC` (`-` for a tensor of no dimensions). A
GGUF that has `general.architecture` starts with the line
`architecture: <it>`; of these only the Qwen3-VL file does (the two image
GGUFs hold no metadata at all). The suffix before `.tsv` is the source's own
extension, which is how a test knows the table's format.

The sources are **not in this repository** (about 50 GB). They were the files
under `~/.local/share/sd_models` on the owner's machine, downloaded from the
Hugging Face repositories below; the sha256 is the LFS hash Hugging Face
recorded for each download (its `.cache/huggingface/download/*.metadata`),
and the header end is where the tensor table stops.

| Golden | Source repository and path | Size (bytes) | Header end | Tensors | sha256 |
|---|---|---|---|---|---|
| `flux1-schnell-q8_0.gguf.tsv` | `leejet/FLUX.1-schnell-gguf` `flux1-schnell-q8_0.gguf` | 12,801,721,248 | 53,912 | 776 | `098c424cfa97526d9227da984563ad1509298f7573458ed210cd5ba0220ab635` |
| `qwen_image_2.1-Q8_0.gguf.tsv` | `leejet/Qwen-Image-2.1-GGUF` `qwen_image_2.1-Q8_0.gguf` | 7,687,155,744 | 20,499 | 265 | `f8b244b00937f0e444a40dbf7866460871b89b30142594973b6012d1b471dc0a` |
| `sd_xl_base_1.0.safetensors.tsv` | `stabilityai/stable-diffusion-xl-base-1.0` `sd_xl_base_1.0.safetensors` | 6,938,078,334 | 402,444 | 2,515 | `31e35c80fc4829d14f90153f4c74cd59c90b779f6afe05a74cd6120b893f7e5b` |
| `ae.safetensors.tsv` | `black-forest-labs/FLUX.1-schnell` `ae.safetensors` (byte-identical at `unsloth/FLUX.1-schnell`) | 335,304,388 | 25,656 | 244 | `afc8e28272cd15db3919bacdb6918ce9c1ed22e96cb12c4d5ed0fba823529e38` |
| `clip_l.safetensors.tsv` | `comfyanonymous/flux_text_encoders` `clip_l.safetensors` | 246,144,152 | 23,192 | 196 | `660c6f5b1abae9dc498ac2d21e1347d2abdb0cf6c0c0c8576cd796491d9a6cdd` |
| `t5xxl_fp16.safetensors.tsv` | `comfyanonymous/flux_text_encoders` `t5xxl_fp16.safetensors` | 9,787,841,024 | 27,136 | 220 | `6e480b09fae049a72d2a8c5fbccb8d3e92febeb233bbe9dfe7256958a9167635` |
| `qwen_image_2.1_vae_bf16.safetensors.tsv` | `Comfy-Org/Qwen-Image-2.1` `vae/qwen_image_2.1_vae_bf16.safetensors` | 675,509,688 | 28,880 | 238 | `bb21f7473051e1ac368515dd3f2e15cd44d7a11748ee8823e1ddca3e4876b7c9` |
| `Qwen3VL-8B-Instruct-Q8_0.gguf.tsv` | `Qwen/Qwen3-VL-8B-Instruct-GGUF` `Qwen3VL-8B-Instruct-Q8_0.gguf` | 8,709,519,456 | 5,957,704 | 399 | `0d264b3941185d00a74f75c4245521dae088ff1efc90ab8d1754e83f5844adb0` |

They were cut with the script below, saved outside the repository and run
read-only over the sources with `python3 -I cut_goldens.py
~/.local/share/sd_models <this folder>`. It reads header bytes only, never a
tensor's data.

```python
"""Cut one tensor-table golden per measured image-model file.

Reads header bytes only, never a tensor's data. One line per tensor,
sorted: its name, a tab, its shape outermost first as AxBxC, or "-" for a
tensor of no dimensions. A GGUF with general.architecture starts with the
line "architecture: <it>". Usage:
    python3 -I cut_goldens.py <sd_models dir> <output dir>
"""
import json
import os
import struct
import sys

SOURCES = [
    "flux-schnell-gguf/flux1-schnell-q8_0.gguf",
    "qwen-image-2.1/qwen_image_2.1-Q8_0.gguf",
    "sdxl/sd_xl_base_1.0.safetensors",
    "flux-schnell-bfl/ae.safetensors",
    "flux_text_encoders/clip_l.safetensors",
    "flux_text_encoders/t5xxl_fp16.safetensors",
    "qwen-image-2.1/vae/qwen_image_2.1_vae_bf16.safetensors",
    "qwen3vl-8b/Qwen3VL-8B-Instruct-Q8_0.gguf",
]

SCALARS = {0: 1, 1: 1, 2: 2, 3: 2, 4: 4, 5: 4, 6: 4, 7: 1, 10: 8, 11: 8, 12: 8}


def u32(f):
    return struct.unpack("<I", f.read(4))[0]


def u64(f):
    return struct.unpack("<Q", f.read(8))[0]


def string(f):
    return f.read(u64(f)).decode("utf-8")


def value(f, kind):
    if kind == 8:
        return string(f)
    if kind == 9:
        element, count = u32(f), u64(f)
        for _ in range(count):
            value(f, element)
        return None
    f.read(SCALARS[kind])
    return None


def gguf(f):
    version = u32(f)
    assert version in (2, 3), version
    tensors, pairs = u64(f), u64(f)
    architecture = None
    for _ in range(pairs):
        key, kind = string(f), u32(f)
        got = value(f, kind)
        if key == "general.architecture":
            architecture = got
    rows = []
    for _ in range(tensors):
        name = string(f)
        dims = [u64(f) for _ in range(u32(f))]
        u32(f)  # type
        u64(f)  # offset
        rows.append((name, list(reversed(dims))))
    return architecture, rows, f.tell()


def safetensors(f, first):
    length = struct.unpack("<Q", first + f.read(4))[0]
    header = json.loads(f.read(length))
    rows = [(k, v["shape"]) for k, v in header.items() if k != "__metadata__"]
    return None, rows, 8 + length


def main(root, out):
    for rel in SOURCES:
        path = os.path.join(root, rel)
        with open(path, "rb") as f:
            magic = f.read(4)
            if magic == b"GGUF":
                architecture, rows, end = gguf(f)
            else:
                architecture, rows, end = safetensors(f, magic)
        name = os.path.basename(rel)
        lines = [] if architecture is None else ["architecture: " + architecture]
        lines += sorted(n + "\t" + ("x".join(str(d) for d in s) or "-") for n, s in rows)
        with open(os.path.join(out, name + ".tsv"), "w", encoding="utf-8") as g:
            g.write("\n".join(lines) + "\n")
        print(name, os.stat(path).st_size, "header", end, "tensors", len(rows))


if __name__ == "__main__":
    main(sys.argv[1], sys.argv[2])
```

<!-- module-docs:end -->
