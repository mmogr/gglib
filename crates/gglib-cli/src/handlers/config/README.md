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
app is never launched. `paths` also prints `schema_generation`, the newest
library this build opens, and `copy_generation` reads that line from
another copy from 0.38.0 on, for `hand_over`.

`path_entry` is `config path-entry add|remove`, hidden from `--help`: the
desktop app's Windows installer runs it to put the folder this `gglib.exe`
is in on the user's own `PATH`, and its uninstaller to take it off if the
install put it there (`gglib_core::paths::user_path`). It runs before
logging is set up (`dispatch::leaves_nothing_behind`), so it makes no data
folder and no log file, and it prints one line, or one line of error.

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

`repair` is the question `install`, `ensure_installed` and `gglib up` ask
when an engine does not work for want of Microsoft's Visual C++ runtime,
which gglib can install: asked in a terminal, answered ahead of time by
`gglib up --yes` (`--force` is never a yes to it), and with nobody to ask
nothing is installed and the line names what to run.

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
