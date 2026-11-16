# Check Dependencies

<!-- module-docs:start -->

System dependency checking module for the CLI.

## Purpose

This module handles the `check-deps` command. It prints the dependency
report, grouped by what each dependency is for, and fails while something
this gglib needs is missing, after printing platform-specific installation
instructions for it.

## Architecture

```text
┌──────────────────────────────────────────────────────────────┐
│                    check_deps Module                         │
├──────────────────────────────────────────────────────────────┤
│                                                              │
│  User Command → Handler → Probe → Display → Instructions    │
│    (execute)    (mod.rs)  (runtime) (display) (instructions)│
│                                                              │
└──────────────────────────────────────────────────────────────┘
```

## Components

- **`mod.rs`** - `execute()`: asks the probe for the report, prints it, and
  decides the exit code from what this gglib needs.
- **`display.rs`** - `render_report()` lays the report out as text: each
  group's heading and sentence, its rows, the verdict. `print_gpu_status()`
  shows GPU detection. Every word comes from the report.
- **`instructions/`** - Platform-specific install commands for what is
  missing (Homebrew, apt, dnf, pacman, zypper, winget).
- **`platform.rs`** - The Linux distribution, re-exported from
  `gglib_runtime`. The OS is `gglib_core::utils::system::HostOs`.

## The report

The report is `gglib_core::utils::system::dependency_report`, a pure function
of the OS, whether this gglib is an installed copy or a build from a checkout
that is still on the machine, and what the probe found. The table of
dependencies, with the names each is looked for under on each OS, is
`gglib_core::utils::system::NEEDS`. It has three groups:

- **Running gglib** - the llama.cpp and stable-diffusion.cpp engines, which
  gglib installs itself; python3, for the optional `hf_xet` accelerator; and
  the GPU runtime found (Metal, CUDA, Vulkan). Nothing here fails the check.
- **Building gglib from source** - cargo, rustc, node, npm (`npm.cmd` on
  Windows), and on Linux the libraries the desktop app links. Needed by a
  checkout build only.
- **Building an engine from source** - git, cmake, a C++ compiler, and on
  macOS and Linux make, pkg-config and OpenSSL (with libcurl on Linux); with
  a Vulkan GPU, Vulkan's headers, glslc and the SPIR-V headers. Needed by a
  checkout build, and by an installed copy whose next llama.cpp install is
  a source build (no pre-built llama.cpp for the machine, or one already
  built from source), as the engine's own install plan decides.

A row marked `*` is needed here; one not needed that is absent reads
`not installed` rather than `missing`. The desktop app's diagnostics show the
same groups, sentences and state words.

## Output Format

An installed copy on Windows with none of the build tools:

```text
Checking system dependencies...

DEPENDENCY           STATUS          NOTES
=====================================================================================

Running gglib
What gglib uses as it runs. It installs its engines itself; the rest is optional.
 llama.cpp           ✓ installed      Runs chat models. gglib installs it when a model is first served
 ...

Building gglib from source
These build gglib from its source code. An installed gglib needs none of them.
 cargo               ○ not installed  Builds gglib's Rust code
 ...

=====================================================================================
✓ Nothing this gglib needs is missing.
```

## Error Handling

The command returns:
- **Exit code 0** - Nothing this gglib needs is missing
- **Exit code 1** - Something it needs is missing; install instructions are printed

`make check-deps` (and so `make setup`) ends with this command's status once
a gglib is built, and a checkout build needs its build tools, so a missing
one still stops the setup there.

## Testing

`gglib_core::utils::system::report`'s tests run every OS's report, installed
and checkout, on every OS. `display_tests.rs` checks the printed text; the
crate's `tests/check_deps_fixture.rs` runs the command against a fixture
`PATH` (a checkout build, since a test binary is one).

<!-- module-docs:end -->
