# paths

<!-- module-docs:start -->

Path utilities for gglib data directories and user-configurable locations.

This module provides the canonical path resolution for all gglib components:
- A path's canonical form (`canonical`)
- Database location
- Models directory
- Llama.cpp binaries (`.llama/`)
- stable-diffusion.cpp's `sd-server` (`.sd/`)
- Application data and resource roots

# One data root for every build

Every gglib build keeps its state in one per-user folder: `~/.local/share/gglib`
on Linux, `~/Library/Application Support/gglib` on macOS, `%LOCALAPPDATA%\gglib`
on Windows. That holds the database, the `.env` overrides file, the daemon's
token and lock, pidfiles, logs, device keys, and the llama.cpp and sd-server
installs. A packaged install and a build from a checkout share it, so the
desktop app, the CLI and the daemon agree whichever binary runs, and moving
or cleaning a checkout changes nothing.

`GGLIB_DATA_DIR` names another folder, and `GGLIB_RESOURCE_DIR` moves the
llama.cpp and sd-server installs alone. A build with a folder of its own also
wants a daemon of its own, on the port `GGLIB_DAEMON_PORT` names.

`source_checkout` says whether this binary was built from a checkout that is
still there. It decides nothing about data: only whether
`gglib config check-deps` holds this gglib to what building it needs (an
installed copy needs none of it), and where `gglib gui` looks for an app
built beside it.

The database records a schema generation, and a build refuses a file a newer
build has written (`gglib-db`'s `setup_generation.rs`), so sharing one library
across versions fails loudly rather than corrupting it.

# Canonical paths

`canonical` is the one way gglib resolves a path, links and `..` included;
`clippy.toml` refuses `std::fs::canonicalize`, `Path::canonicalize` and
`tokio::fs::canonicalize` everywhere else. What it returns is what the
library stores (a model's files, projector and components, a chat's folder),
what is compared, what is shown and what an engine is given, on every OS.

On Windows `canonicalize` gives a verbatim path, `\\?\C:\Users\...` or
`\\?\UNC\server\share\...`. Many programs refuse that form, and it names a
file by a second string. `canonical` writes it as the plain path,
`C:\Users\...` or `\\server\share\...`, whenever the plain path names the
same file: it is at most 259 UTF-16 units long (`MAX_PATH`, 260, counts the
terminating NUL), and no name in it ends in a dot or a space, holds a
character a plain path reads (`<>:"/|?*` or one below U+0020), is `.` or
`..`, or is a reserved device name (`CON`, `NUL.txt`, `COM1` and the rest). A
drive's or a share's root keeps its one trailing separator (`C:\`,
`\\server\share\`). Any other verbatim path is kept as it is, since it can be
reached in no other way. The rule reads only the path's text, so its tests
run on every OS; on macOS and Linux a canonical path never has the prefix.

So a canonical folder may be plain while a file in it is not: `notes.`,
`aux.c` or a path past `MAX_PATH` inside `C:\proj` keeps the prefix. Two
functions keep that exact. `child` gives an entry's canonical path in a
canonical folder, and `join` joins a relative path onto a folder, each in the
verbatim form when a name needs it, since a plain `C:\proj\notes.` would open
`C:\proj\notes`. `inside` says whether a canonical path lies in a canonical
folder, and what part of it is under the folder, for either form of each;
`inside_on` is the same, compared as an OS (a `HostOs` value) compares
names, without case on Windows and macOS, and `drive_root` reads the root of
the drive a Windows path names (`C:\` for `C:\Windows`), both from the text
alone, so a chat's system folders (`domain::folder::Places`) are compared
alike on every OS.
`is_verbatim` says whether a string has the prefix, for the one check that
refuses a library an older build wrote on Windows.

# The models directory

`set_models_dir` is the one way a surface changes it: it resolves the path,
creates the directory and stores it in the data root's `.env`, quoted, so
that a path with a space in it is still a line that file's loader reads.
The daemon's first start stores the default the same way
(`store_default_models_dir`), unless `.env` already has a line for it.
`resolve_models_dir` reads it back: an explicit path first, then
`GGLIB_MODELS_DIR` in the environment, then the stored directory, read with
the loader's own parser, then the default. A daemon's download manager asks
it as each download starts and keeps the answer for that download's files
alone, so a directory stored while the daemon runs is where its next download
goes, unless the daemon's own environment names one.
`can_write_folder` says whether it can be written by writing a probe file
to it, or, while it is not there, to the nearest folder above it that is:
the settings page's, the setup status's and `gglib config show`'s
state for the folder.

# The user's own PATH

`user_path` puts a folder on the user's own `PATH`, and takes it off, for
the desktop app's Windows installer, which runs `gglib config path-entry
add` after it installs and `remove` before it uninstalls, so `gglib` runs in
a terminal opened afterwards. The folder is the one the running `gglib.exe`
is in.

`remove` takes off only what `add` put on. A folder already on `PATH`,
because the user installed into a folder of their own tools or added it by
hand, is left there by both. `add` records each folder it adds in
`HKEY_CURRENT_USER\Software\gglib`, value `PathEntriesAdded`, a list
separated by `;`, writing the record before `PATH`; `remove` acts only on a
folder the record names, writing `PATH` before the record, and drops the
folder from the record whether or not the entry was still there. A crash
between two writes so leaves at worst a record naming a folder not on
`PATH`, which the next install completes and the next uninstall clears.

`with_folder` adds the folder once, as the last entry, or as the first when
the value ends inside an unclosed quote; `without_folder` takes out every
entry that names it. Entries are split at each `;` outside double quotes,
and an unclosed quote runs to the end. An entry names the folder when the
two are the same text once surrounding quotes and trailing separators are
dropped, `/` is read as `\` and case is ignored, so `C:\a\gglib-gui-old` is
another folder. Every other entry is kept as it is written, `%VAR%`
references and quotes included, and removing what was added gives back the
value as it was; a value left with nothing in it is stored empty. The value
is read and written whole, keeping its `REG_EXPAND_SZ` or `REG_SZ` type, by
`utils::process::host`, the one file that calls Windows; macOS and Linux
keep no such variable, and both edits change nothing there.

# Design

- Returns `PathBuf` and `PathError` for clear error handling
- No interactive/terminal I/O - adapters handle user prompts separately
- OS-specific logic is kept private in `platform`

<!-- module-docs:end -->
