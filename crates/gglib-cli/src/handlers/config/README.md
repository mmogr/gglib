# config

<!-- module-docs:start -->

Configuration, tooling, and system management handlers.

Dispatches [`ConfigCommand`] variants to focused sub-modules:
settings (`show`, `set`, `unset`, `reset`, `reveal`, profiles), llama.cpp
lifecycle, dependency checks, the optional download accelerator, and resolved-path
inspection.

`paths` prints every resolved path, then `copies`' lines: each `gglib` on
`PATH` in the order a shell tries them, which one `gglib` runs and which
one is running, and the desktop app, each with its version. Another copy
is asked only `--version`, by `copy_version`, with a two-second wait; the
app is never launched.

The two engines, llama.cpp and stable-diffusion.cpp, are installed the
same way, by `engine`: `install`, which `update` is too, installs the pinned
release when what is there is missing or older, and is refused while the
daemon that serves this data root runs a model on that engine; `uninstall`
removes what an install put there, refused the same way; `ensure_installed` is the offer `gglib serve`, `gglib model load` and `gglib up` make before
they need an engine, of the install when it is missing and of the pin when
what is there is older. Only a terminal is asked: with no terminal and no
`--yes`, stdin is not read, an update is kept back and a missing engine
refused. Whether there is anything to install, and whether it
is a download or a source build, are `gglib_runtime::engine`'s to say.

What is each engine's own: `llama` routes llama.cpp's subcommands, and
`llama_install` runs its download and its source build, with the build's
own checks and question. `sd` routes stable-diffusion.cpp's and runs its
download and build, and its `status`. `llama_events` draws the
progress of both, and is the only place a build or a download is rendered
for a terminal.

`check_deps` reports and installs nothing. `fast_downloads` provisions
only on an explicit `enable`, or when the user accepts the offer its
`prompt` subcommand makes from `make setup` and `gglib up`.

<!-- module-docs:end -->
