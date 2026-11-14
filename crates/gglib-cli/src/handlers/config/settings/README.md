# settings

<!-- module-docs:start -->

Handlers for `gglib config show`, `set`, `unset`, `reset`, `reveal` and
`profile`.

`set` and `unset` take a key from `config_commands::SettingKey`'s table and
write it through the wire type the settings API reads (`wire`), so the CLI
and the app change a setting by one path. `models-dir` is the one key that is
not a setting: it is a line in the data root's `.env` (`models_dir`).

All display/formatting logic is in the sibling [`settings_display`] module.

A command stores settings in its own process, and the daemon reads most of
them from the store as it uses them. Its download manager holds the queue
size it was given, so `set`, `unset` and `reset` send the daemon that serves
this library the size they stored (`queue_size`); a daemon that cannot be
told is a warning, and it takes the stored size when it next starts.

<!-- module-docs:end -->
