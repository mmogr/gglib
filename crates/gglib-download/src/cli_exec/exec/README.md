# exec

<!-- module-docs:start -->

The optional `hf_xet` download accelerator: the Python environment that backs
it, and the subprocess bridge that drives it. Kept separate from queue
management, and from the default download path — that is `crate::executor`,
which is native Rust and reaches this module only when
[`python_env::fast_helper_provisioned`] says the environment is already here.

**Nothing in this module runs implicitly.** `python_env.rs` builds the
environment, but `PythonEnvironment::prepare` is reachable only from
`ensure_fast_helper_ready`, which in turn is only called by an explicit opt-in:
`gglib config fast-downloads enable`, the offer `make setup` and `gglib up`
make, or the GUI setup wizard. A download never provisions anything; if the
environment is absent, the native path runs instead. Putting a Python toolchain
in the critical path of a first download is the failure mode this arrangement
exists to prevent.

The environment lives at `<data_root>/.python/gglib-hf-xet`, or at the
pre-rename `<data_root>/.conda/gglib-hf-xet` when an older install already has
one there. It is built with `uv` when that is installed and `python -m venv`
plus pip otherwise; both produce the same `bin/python` layout, so nothing
downstream depends on which ran. The interpreter it is seeded from is found by
searching `PATH` (including versioned names), an active conda prefix, the
conda-family home layouts, and the pyenv and uv version stores — but whatever
Python or environment manager the user has active is scrubbed from every child
process. gglib reads those variables to locate an interpreter, never to run
inside one.

Its packages are pinned in `scripts/hf_xet_requirements.txt`, which gglib
embeds, to the release lines the helper was tested against. They need Python
3.10. The marker beside the environment records the pins it was installed
with, so a changed pin re-installs on the next use, and an environment whose
interpreter is older than 3.10 is built again first.

`PythonEnvironment::prepare` takes an optional `NoticeCallback`
(`Option<&NoticeCallback>`, aliased in `python_bridge.rs`): with one supplied,
venv creation and dependency install surface as the status of the download's
row instead of a console line;
without one they fall back to
`gglib_core::telemetry::console_println`. A queued download supplies one, and
so does a `model upgrade` that was given a `RowCallback`, as the CLI's is.
`ensure_fast_helper_ready`, the explicit setup, has none, nor has an upgrade
given no `RowCallback`, as the daemon's route runs it.
The environment build runs via
`.output()`, not `.status()`, so its own stdio is captured rather than
inherited — an inherited handle would write straight to the terminal, outside
any bar's bookkeeping, the same way a stray `println!` would. This matters more
with uv, which is chattier than `python -m venv`.

The helper (`scripts/hf_xet_downloader.py`) is started once per file. It reads
the Hub token from `HF_TOKEN` in its environment, not from its arguments, and
writes its progress as JSON lines: `written`, the bytes of the file on disk,
and `received`, the bytes off the network in this run. A report within 0.2 s
of the last line is not written. When the helper's bar closes it writes the
final count, whatever was dropped before, and `complete` follows. A file the
Hub finds already in place gets no bar, and so no count. Of
its stderr, where its own progress bar draws, only the last 4 KiB are kept,
as the text of a failure. `scripts/test_hf_xet_downloader.py` drives the
helper's bar through the real Hub library; CI runs it against the pinned
packages, and `make test-helper` does where a venv holding them exists.

Nothing in this module draws. `model upgrade` downloads here, without the
download manager, and its files are fetched by `crate::solo`, which makes
the download's row with the queue's row builder and hands it to the
`RowCallback` that `update_model` was given. The CLI draws that row on its
download board. The daemon's upgrade route passes no callback, so its
upgrade's progress is shown nowhere and its setup notes go to the console.

<!-- module-docs:end -->
