# settings

<!-- module-docs:start -->

Settings, default-model, and models-directory handlers.

These three sub-handlers were the original `config` dispatch targets.
They now live inside the `config/` directory alongside llama, check-deps,
and paths.

All display/formatting logic is in the sibling [`settings_display`] module.

A command stores settings in its own process, and the daemon reads most of
them from the store as it uses them. Its download manager holds the queue
size it was given, so `set`, `unset` and `reset` send the daemon that serves
this library the size they stored (`queue_size`); a daemon that cannot be
told is a warning, and it takes the stored size when it next starts.

<!-- module-docs:end -->
