# `config_commands`

<!-- module-docs:start -->

Clap definitions for `gglib config`: `show`, `set`, `unset`, `reset` and
`reveal` for settings, profiles, paths, the llama.cpp toolchain,
stable-diffusion.cpp (the image runtime) and dependency checks.

`SettingKey` is the key `set` and `unset` take: one row per key in
`setting_key.rs`, named as `show` prints it, with what its value is. Each
command takes the rows it can change, and its `--help` lists them and names
the others with where they are changed; clap refuses any other key. The table
is also what `scripts/check_settings_surfaces.sh` reads to prove no
`Settings` field is stranded without a way to set it.

`SecretArg` is the key `reveal` takes: core's `SettingSecret`, named on the
command line by `SettingSecret::name`, so `--help`, completions and the
refusal of an unknown key read the one spelling of each name.

<!-- module-docs:end -->
