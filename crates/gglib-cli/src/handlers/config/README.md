# config

<!-- module-docs:start -->

Configuration, tooling, and system management handlers.

Dispatches [`ConfigCommand`] variants to focused sub-modules:
settings/default/models-dir, llama.cpp lifecycle, dependency
checks, the optional download accelerator, and resolved-path
inspection.

`check_deps` reports and installs nothing. `fast_downloads` is the one
handler here that provisions anything, and only on an explicit
`enable`, or when the user accepts the offer its `prompt` subcommand
makes from `make setup` and `gglib up`.

<!-- module-docs:end -->
