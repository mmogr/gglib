# config_commands

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-config_commands-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-config_commands-complexity.json)

<!-- module-docs:start -->

Clap definitions for `gglib config` — settings, inference defaults, profiles,
paths, models-directory and dependency checks.

`SettingsSetArgs` lives in its own module rather than inline in the
`SettingsCommand::Set` variant. It is a third of this definition on its own and
grows with every setting — one flag, one doc comment, one `#[arg]` each — so
adding a setting touches a file about settings rather than the file that
enumerates every config subcommand. It is also the list
`scripts/check_settings_surfaces.sh` reads to prove no `Settings` field is
stranded without a way to set it.

<!-- module-docs:end -->

<details>
<summary><h2>Modules</h2></summary>

<!-- module-table:start -->
| Module | LOC | Complexity | Coverage |
|--------|-----|------------|----------|
| [`settings_args.rs`](settings_args.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-config_commands-settings_args-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-config_commands-settings_args-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-config_commands-settings_args-coverage.json) |
<!-- module-table:end -->

</details>
