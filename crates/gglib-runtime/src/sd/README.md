# Stable Diffusion

<!-- module-docs:start -->

Installing stable-diffusion.cpp's `sd-server`, the image runtime, reporting
what is installed, and its command line.

Everything lives under `.sd/` beside llama.cpp's `.llama/` (paths in
`gglib_core::paths`): `bin/sd-server` and the shared library it loads from
beside itself (`libstable-diffusion.dylib`, `.so`, or `stable-diffusion.dll`),
`sd-config.json`, a source checkout, and `downloads/` while an archive is
unpacked. Uninstalling removes `.sd/` whole.

| File          | What it holds                                                            |
|---------------|--------------------------------------------------------------------------|
| `release.rs`  | the pin `master-948-228c707`, `GGLIB_SD_RELEASE`, the platform table     |
| `install.rs`  | the pre-built install, and the source build of the `sd-server` target    |
| `record.rs`   | `sd-config.json`: a source build's five keys or a download's four        |
| `status.rs`   | `SdStatus`, from the record and the binary's own `--version`             |
| `uninstall.rs`| removing `.sd/`                                                           |
| `config.rs`   | `SdServerConfig`: the model, its family, its components and the port     |
| `args.rs`     | `sd-server`'s argv from the config and the family's recipe               |
| `spawn.rs`    | starting it, with piped output, as llama-server is started               |
| `job.rs`      | `SdImageDriver`, the image generation port: admit, turn, submit, follow  |
| `job_plan.rs` | a request's model, size and count, refused before anything queues        |
| `job_api.rs`  | sd-server's async job API: submit, read a job, cancel                     |
| `job_poll.rs` | following one job: stages, steps, decode, cancel, stall and deadline     |

## The platform table

Asset names carry the build runner's OS version
(`sd-master-228c707-bin-Darwin-macOS-26.6.2-arm64.zip`), so each platform is
a pattern, and exactly one asset must match it.

| Platform        | Asset pattern                         | Must hold                                  |
|-----------------|---------------------------------------|--------------------------------------------|
| macOS, any arch | `-bin-Darwin-macOS-` … `-arm64.zip`   | `sd-server`, `libstable-diffusion.dylib`   |
| Linux `x86_64`  | `-bin-Linux-` … `-x86_64-vulkan.zip`, or `-x86_64.zip` with no Vulkan runtime (a warning) | `sd-server`, `libstable-diffusion.so` |
| Windows `x86_64`| `-bin-win-cuda12-`, `-bin-win-vulkan-` or `-bin-win-cpu-` … `-x64.zip` by GPU (the CPU build with a warning), plus the CUDA runtime package for CUDA | `sd-server.exe`, `stable-diffusion.dll` |

The macOS asset is a universal binary named after its arm64 runner, and its
`sd-server` finds the dylib through `@executable_path`. Any other platform
has no asset and builds from source.

## The command line

`--diffusion-model <file>` for a diffusion-only family (Flux.1, Qwen-Image
2.1) or `-m <file>` for an all-in-one checkpoint (SDXL); each component in
role order, `--vae`, `--clip_l`, `--t5xxl`, `--llm`, whatever order the
model lists them in; `--listen-ip 127.0.0.1 --listen-port <port>`; then the
recipe's `--steps`, `--cfg-scale` and `--sampling-method`, and `--fa` where
the recipe turns flash attention on (Qwen-Image 2.1). Flags given at launch
are the defaults for every request. `args_tests.rs` pins one whole argv per
family.

## Launching

The residency launch starts `sd-server` (`process::residency`): it checks
the binary and every component before the queue, places the model by its
files plus its family's compute margin (`Recipe::compute_margin_bytes`, the
1024x1024 VAE decode buffer rounded up: Flux.1 7 GiB, SDXL 8 GiB,
Qwen-Image 2.1 9 GiB), spawns with `spawn.rs`, and narrates the build from
`sd-config.json` (`recorded_release`).

## Is it up?

`sd-server` has no `/health`. Readiness and the health monitor ask
`/v1/models` instead (`RuntimeKind::health_path`), which answers without the
lock a render holds, and the body must list `sd-cpp-local`: a 200 from any
other server on the port is not sd-server. `health_tests.rs` holds a render
open on `fake_server.rs`, a test-only stand-in, and probes three times
within the health client's two seconds.

## Drawing

`SdImageDriver` implements core's `ImageGenerationPort`; the service graph
builds one over the daemon's manager. A request names its model, or draws
with the default image model the settings name (refused when it lacks a
file), or else with the only image model that has every file its family
needs; the
family's recipe judges its size (1024x1024 by default) and one to four
images, all before anything queues. Then the image model is admitted
(launching sd-server if it is not resident, its place in line reported as
Queued), the render takes its turn on the generation gate with that lease,
and the job goes to `POST /sdcpp/v1/img_gen` with `output_format: png` and
`preview: proj`, since sd-server reports steps only while a preview mode is
on. The job is read every second: Loading until the first step finishes
(about 40 s into a Flux render, measured), then each step as Sampling with
its frame and as progress on the turn, which keeps requests queued behind
the render from expiring, then Decoding after the last step of the last
pass, then the PNGs, their sizes read from the images.

sd-server cannot interrupt a generating job (cancel answers 409). A render
dropped part way cancels its job and, when the job runs on, a task keeps the
turn and its lease until the job ends; the submission runs in a task of its
own, so that holds for a request dropped while its job is being submitted. A render with no new step for
`IMAGE_STALL` (3 minutes, from submission) or running past
`IMAGE_JOB_DEADLINE` (30 minutes) is retired through
`ProcessManager::retire_render`: its server stopped, its lease released and
slot emptied, then its turn ended. `job_tests.rs` drives all of this against
a scripted job and a real admission queue on a paused clock;
`job_api_tests.rs` reads every answer the fake's job API scripts.

## The source build

`git clone --depth 1 --branch <tag> --recurse-submodules
--shallow-submodules`, then `cmake -DCMAKE_BUILD_TYPE=Release
-DSD_SERVER_BUILD_FRONTEND=OFF` with `-DSD_METAL=ON`, `-DSD_CUDA=ON` or
`-DSD_VULKAN=ON`, and `cmake --build --target sd-server`. It links
stable-diffusion.cpp statically, so the one binary is the install. The
events are llama.cpp's `BuildEvent`s.

macOS installs the release asset: measured on 2026-10-10 against a source
build on the same Flux render, one run each, it sampled in 73.47 s against
the source build's 82.06 s (log-0017).

<!-- module-docs:end -->
