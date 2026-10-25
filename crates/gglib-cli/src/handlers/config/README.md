# config

<!-- module-docs:start -->

Configuration, tooling, and system management handlers.

Dispatches [`ConfigCommand`] variants to focused sub-modules:
settings/default/models-dir, llama.cpp lifecycle, dependency
checks, the optional download accelerator, and resolved-path
inspection.

The two engines, llama.cpp and stable-diffusion.cpp, are installed the
same way, by `engine`: `install`, which `update` is too, installs the pinned
release when what is there is missing or older, and is refused while the
daemon that serves this data root runs a model on that engine;
`ensure_installed` is the offer `gglib serve` and `gglib up` make before
they need an engine, of the install when it is missing and of the pin when
what is there is older. Whether there is anything to install, and whether it
is a download or a source build, are `gglib_runtime::engine`'s to say.

What is each engine's own: `llama` routes llama.cpp's subcommands, and
`llama_install` runs its download and its source build, with the build's
own checks and question. `sd` routes stable-diffusion.cpp's and runs its
download and build, its `status`, and its `uninstall`, refused while an
image model runs; `sd_prompt` is the install `make setup` offers, which a
no or a failure leaves as a skipped step. `llama_events` draws the
progress of both, and is the only place a build or a download is rendered
for a terminal.

`check_deps` reports and installs nothing. `fast_downloads` provisions
only on an explicit `enable`, or when the user accepts the offer its
`prompt` subcommand makes from `make setup` and `gglib up`.

<!-- module-docs:end -->
