# system

<!-- module-docs:start -->

System probe implementation for gglib-runtime.

This module provides the `DefaultSystemProbe` which implements
`SystemProbePort` from gglib-core. It performs active system probing
via command execution and hardware detection.

`host.rs` reads this machine's name and the free space on a disk with
sysinfo. `host_facts()` hands both readers to the proxy, which has no host
probe of its own, and `this_machine_name()` is also what a pairing tells the
other machine this one is called.

<!-- module-docs:end -->
