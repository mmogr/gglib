# config

<!-- module-docs:start -->

Configuration, tooling, and system management handlers.

Dispatches [`ConfigCommand`] variants to focused sub-modules:
settings/default/models-dir, llama.cpp lifecycle, dependency
checks, the optional download accelerator, and resolved-path
inspection.

The llama.cpp lifecycle is six files. `llama` routes the subcommands.
`llama_method` chooses between a download and a source build, and
`llama_install` is the command that carries the choice out, with the source
build's own checks and question; `llama_ensure` offers that same install to
`gglib serve` and `gglib up` when they find no `llama-server`. `llama_update`
runs an update that `gglib_runtime::llama::update_preflight` has allowed.
`llama_events` draws the progress of all three, and is the only place a
build or a download is rendered for a terminal.

`sd` is the image runtime's twin, stable-diffusion.cpp's `sd-server`:
`install` (a download, or a source build on `--build` or where no release
fits), `status` and `uninstall` (refused while the daemon that serves this
data root runs an image model); `sd_ensure` is the install `gglib serve`
offers when it is asked for an image model and finds no `sd-server`. Both
draw with `llama_events` too, under stable-diffusion.cpp's name.

`check_deps` reports and installs nothing. `fast_downloads` is the one
handler here that provisions anything, and only on an explicit
`enable`, or when the user accepts the offer its `prompt` subcommand
makes from `make setup` and `gglib up`.

<!-- module-docs:end -->
