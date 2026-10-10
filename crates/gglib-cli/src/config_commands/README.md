# `config_commands`

<!-- module-docs:start -->

Clap definitions for `gglib config` — settings, inference defaults, profiles,
paths, models-directory, the llama.cpp toolchain, stable-diffusion.cpp (the
image runtime) and dependency checks.

`SettingsSetArgs` lives in its own module rather than inline in the
`SettingsCommand::Set` variant. It is a third of this definition on its own and
grows with every setting — one flag, one doc comment, one `#[arg]` each — so
adding a setting touches a file about settings rather than the file that
enumerates every config subcommand. It is also the list
`scripts/check_settings_surfaces.sh` reads to prove no `Settings` field is
stranded without a way to set it.

<!-- module-docs:end -->
