# variants_fixtures

<!-- module-docs:start -->

Fixtures `variants_tests.rs` reads with `include_str!`: eight repositories'
trees as `https://huggingface.co/api/models/<repo>/tree/main?recursive=true`
answered on 2026-11-02, each named `<owner>_<repo>.json` and kept to each
entry's type, path and size. Together they hold a quant line per file
(bartowski, mradermacher), Unsloth Dynamic labels beside plain ones, a
folder of shards per variant (`unsloth/DeepSeek-V3.1-GGUF`), drafts and MTP
heads beside their models (HauhauCS, Mia-AiLab) and a repository of one
file (`google/gemma-3-1b-it-qat-q4_0-gguf`).

<!-- module-docs:end -->
