# system

<!-- module-docs:start -->

System probe implementation for gglib-runtime.

This module provides the `DefaultSystemProbe` which implements
`SystemProbePort` from gglib-core. It performs active system probing
via command execution and hardware detection.

`dependency_report()` looks for each row of gglib-core's dependency table
(`utils::system::NEEDS`) under the names that row has on this OS, through the
time-limited helpers in `llama::detect::tools` (`deps.rs`), adds the GPU,
the installed engines and how llama.cpp's next install would be made (the
engine's own `plan_install`), and hands that to the pure `dependency_report` in gglib-core. This
is an installed copy unless `gglib_core::paths::source_checkout` finds the
checkout it was built from.

`host.rs` reads this machine's name and the free space on a disk with
sysinfo. `host_facts()` hands both readers to the proxy, which has no host
probe of its own, and `this_machine_name()` is also what a pairing tells the
other machine this one is called.

`vulkan_devices.rs` decides whether this machine has a GPU a Vulkan build
runs on, for every report of a Vulkan GPU and for both engines' choice
between their Vulkan and CPU builds. It reads the devices
`vulkaninfo --summary` lists, and a software rasteriser (llvmpipe,
`SwiftShader`, Windows' Basic Render Driver) is no GPU. Where no list can be
read it asks the operating system: on Linux a GPU device node and a Vulkan
driver's ICD manifest, on Windows a display adapter with a Vulkan driver
registered. The Vulkan loader alone is not a GPU.

<!-- module-docs:end -->
