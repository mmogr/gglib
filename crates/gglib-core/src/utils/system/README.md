# system

<!-- module-docs:start -->

System utility types for dependency and environment detection.

This module provides pure domain types for system dependencies,
GPU information, and memory details. Active system probing is
implemented by `DefaultSystemProbe` in `gglib-runtime`.

# Architecture Note

Core defines types + the `SystemProbePort` trait (in `ports::system_probe`).
Runtime implements `DefaultSystemProbe` which performs actual system queries.

`packages` follows the same split, which is what keeps it here rather than in
the runtime: `parse_os_release` takes the *contents* of `/etc/os-release` and
returns a `LinuxDistro`, so the decision is pure and testable against files
from distributions no CI machine runs, while the `std::fs` call that produces
those contents lives in `gglib_runtime::system::detect_linux_distro`.

`needs` is the dependency table: each row's purpose (running gglib, building
it from source, building an engine from source), whether that purpose
requires it, and the names it is looked for under on Linux, macOS and
Windows, so `npm` is `npm.cmd` on Windows and make and pkg-config are not
looked for there. An OS is a value (`HostOs`), not a `cfg`. `report` makes
the dependency report from the table, the OS, whether this gglib is an
installed copy or a checkout build (`BuildOrigin`) and what a probe found
(`Found`): a pure function, so every OS's report is tested on every OS. An
installed copy needs neither build group, unless llama.cpp's next install
there is a source build (`EngineInstall`: no pre-built llama.cpp for the
machine, or one built from source), when it needs the engine build. The report's titles,
sentences and verdict are what `gglib config check-deps` prints and the
app's diagnostics show.

`packages` is the single source of truth for what each dependency is called
on each distribution — used by the report's install hints, by
`gglib config check-deps`, and by the llama.cpp build's own prerequisite
message, which previously each carried their own copy and had drifted apart.

<!-- module-docs:end -->
