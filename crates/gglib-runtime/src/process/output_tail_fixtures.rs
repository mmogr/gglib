//! What the engines really printed when a load failed, recorded on macOS
//! (Apple M4 Max) on 2026-11-16 from llama-server build 12110 (cd959bae9)
//! and the pinned sd-server, run by hand, with stderr captured (stdout as
//! well for sd-server). Paths are shortened to `/models/…`; nothing else is
//! changed. Each stream went to a file, not a terminal, as a launch reads
//! it through a pipe: llama.cpp marks each line with a time and a level
//! there, and sd-server with `[E]`, `[W]` and `[I]`. The tests in
//! `output_tail_tests.rs` hold the rule for the line
//! that says why to each.

/// llama-server -m (a text file named .gguf): it exited 1.
pub(super) const NOT_A_GGUF: &str = r"0.00.003.290 I srv  llama_server: initializing ...
0.00.123.447 I cmn  common_param: common_params_print_info: verbosity = 3 (adjust with the `-lv N` CLI arg)
0.00.123.640 W srv  server_setup: security: no API key is set and CORS allows all origins (see https://github.com/ggml-org/llama.cpp/pull/25655)
0.00.125.084 I srv    load_model: loading model '/models/notgguf.gguf'
0.00.125.192 E gguf_init_from_reader: invalid magic characters: 'this', expected 'GGUF'
0.00.125.716 E llama_model_load: error loading model: llama_model_loader: failed to load model from /models/notgguf.gguf
0.00.125.833 E llama_model_load_from_file_impl: failed to load model
0.00.126.221 E common_fit_params: encountered an error while trying to fit params to free device memory: failed to load model
0.00.126.267 E gguf_init_from_reader: invalid magic characters: 'this', expected 'GGUF'
0.00.126.301 E llama_model_load: error loading model: llama_model_loader: failed to load model from /models/notgguf.gguf
0.00.126.303 E llama_model_load_from_file_impl: failed to load model
0.00.126.304 E cmn  common_init_: failed to load model '/models/notgguf.gguf'
0.00.126.304 E srv    load_model: failed to load model, '/models/notgguf.gguf'
0.00.126.305 I srv    operator(): operator(): cleaning up before exit...
0.00.126.796 E srv  server_run_i: exiting due to model loading error";

/// llama-server -m (a path with no file): it exited 1.
pub(super) const MISSING_FILE: &str = r"0.00.000.852 I srv  llama_server: initializing ...
0.00.306.092 I cmn  common_param: common_params_print_info: verbosity = 3 (adjust with the `-lv N` CLI arg)
0.00.306.286 W srv  server_setup: security: no API key is set and CORS allows all origins (see https://github.com/ggml-org/llama.cpp/pull/25655)
0.00.308.006 I srv    load_model: loading model '/nonexistent/qwen.gguf'
0.00.308.059 E gguf_init_from_file: failed to open GGUF file '/nonexistent/qwen.gguf' (No such file or directory)
0.00.308.149 E llama_model_load: error loading model: llama_model_loader: failed to load model from /nonexistent/qwen.gguf
0.00.308.358 E llama_model_load_from_file_impl: failed to load model
0.00.308.431 E common_fit_params: encountered an error while trying to fit params to free device memory: failed to load model
0.00.308.441 E gguf_init_from_file: failed to open GGUF file '/nonexistent/qwen.gguf' (No such file or directory)
0.00.308.487 E llama_model_load: error loading model: llama_model_loader: failed to load model from /nonexistent/qwen.gguf
0.00.308.490 E llama_model_load_from_file_impl: failed to load model
0.00.308.491 E cmn  common_init_: failed to load model '/nonexistent/qwen.gguf'
0.00.308.492 E srv    load_model: failed to load model, '/nonexistent/qwen.gguf'
0.00.308.493 I srv    operator(): operator(): cleaning up before exit...
0.00.308.673 E srv  server_run_i: exiting due to model loading error";

/// llama-server -m (a real model) --mmproj (a path with no file): it exited 1 after loading the model; it printed errors for the projector early, carried on, and failed on it again at the end.
pub(super) const MISSING_MMPROJ: &str = r"0.00.000.573 I srv  llama_server: initializing ...
0.00.088.451 I cmn  common_param: common_params_print_info: verbosity = 3 (adjust with the `-lv N` CLI arg)
0.00.088.648 W srv  server_setup: security: no API key is set and CORS allows all origins (see https://github.com/ggml-org/llama.cpp/pull/25655)
0.00.090.385 E gguf_init_from_file: failed to open GGUF file '/nonexistent/mmproj.gguf' (No such file or directory)
0.00.090.799 E mtmd_get_cap_from_file: failed to get capabilities from file '/nonexistent/mmproj.gguf': clip_model_loader: failed to load CLIP model from /nonexistent/mmproj.gguf. Does this file exist?

0.00.090.907 I srv    load_model: loading model '/models/unsloth_Qwen3-4B-Instruct-2507-GGUF/Qwen3-4B-Instruct-2507-Q8_0.gguf'
0.00.090.921 E gguf_init_from_file: failed to open GGUF file '/nonexistent/mmproj.gguf' (No such file or directory)
0.00.090.983 E mtmd_get_memory_usage: error: Failed to load CLIP model from /nonexistent/mmproj.gguf

0.00.090.984 E srv    load_model: [mtmd] failed to get memory usage of mmproj
0.00.532.944 W load: control-looking token: 128247 '</s>' was not control-type; this is probably a bug in the model. its type will be overridden
0.07.234.435 I cmn          init: llama threadpool init, n_threads = 12
0.07.513.402 E gguf_init_from_file: failed to open GGUF file '/nonexistent/mmproj.gguf' (No such file or directory)
0.07.513.457 E clip_init: failed to load model '/nonexistent/mmproj.gguf': clip_model_loader: failed to load CLIP model from /nonexistent/mmproj.gguf. Does this file exist?

0.07.513.485 E mtmd_init_from_file: error: Failed to load CLIP model from /nonexistent/mmproj.gguf

0.07.513.486 E srv    load_model: failed to load multimodal model, '/nonexistent/mmproj.gguf'
0.07.513.487 I srv    operator(): operator(): cleaning up before exit...
0.07.513.815 E srv  server_run_i: exiting due to model loading error";

/// llama-server --port (a port another program holds): it exited 1.
pub(super) const PORT_IN_USE: &str = r"0.00.000.870 I srv  llama_server: initializing ...
0.00.089.761 I cmn  common_param: common_params_print_info: verbosity = 3 (adjust with the `-lv N` CLI arg)
0.00.089.998 W srv  server_setup: security: no API key is set and CORS allows all origins (see https://github.com/ggml-org/llama.cpp/pull/25655)
0.00.090.090 E srv         start: couldn't bind HTTP server socket, hostname: 127.0.0.1, port: 9887
0.00.090.093 I srv    operator(): operator(): cleaning up before exit...
0.00.090.195 E srv  server_run_i: exiting due to HTTP server error";

/// llama-server --bogus-flag: it exited 1, on one unmarked line.
pub(super) const BAD_FLAG: &str = r"error: invalid argument: --bogus-flag";

/// llama-server -m (a real model) -c 2000000 -ctk f32 -ctv f32 -fit off: still running after 40 s, and stopped by hand. Its last line is a warning.
pub(super) const STILL_LOADING: &str = r"0.00.000.464 I srv  llama_server: initializing ...
0.00.055.678 I cmn  common_param: common_params_print_info: verbosity = 3 (adjust with the `-lv N` CLI arg)
0.00.055.882 W srv  server_setup: security: no API key is set and CORS allows all origins (see https://github.com/ggml-org/llama.cpp/pull/25655)
0.00.057.154 I srv    load_model: loading model '/models/unsloth_Qwen3-4B-Instruct-2507-GGUF/Qwen3-4B-Instruct-2507-Q8_0.gguf'
0.00.101.584 W load: control-looking token: 128247 '</s>' was not control-type; this is probably a bug in the model. its type will be overridden
0.00.300.126 W llama_context: n_ctx_seq (2000128) > n_ctx_train (262144) -- possible training context overflow";

/// sd-server --diffusion-model (a text file named .gguf): it exited 1. Its last ten Metal lines on stderr, then its three lines on stdout, then its two on stderr; the streams were captured apart, so their order between them is the one they are read in when stdout's arrive first.
pub(super) const SD_NOT_A_GGUF: &str = r"ggml_metal_device_init: GPU family: MTLGPUFamilyApple9  (1009)
ggml_metal_device_init: GPU family: MTLGPUFamilyMetal4  (5002)
ggml_metal_device_init: simdgroup reduction   = true
ggml_metal_device_init: simdgroup matrix mul. = true
ggml_metal_device_init: has unified memory    = true
ggml_metal_device_init: has bfloat            = true
ggml_metal_device_init: has tensor            = false
ggml_metal_device_init: use residency sets    = true
ggml_metal_device_init: use shared buffers    = true
ggml_metal_device_init: recommendedMaxWorkingSetSize  = 115448.73 MB
[I] loading diffusion model from '/models/notgguf.gguf' --- diffusion_engine.cpp:692
[W] unknown format /models/notgguf.gguf --- model_loader.cpp:232
[W] loading diffusion model from '/models/notgguf.gguf' failed --- diffusion_engine.cpp:694
[E] get sd version from file failed: '' --- diffusion_engine.cpp:972
[E] new_sd_ctx_t failed --- main.cpp:105";
