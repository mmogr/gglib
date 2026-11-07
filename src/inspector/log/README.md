# log

<!-- module-docs:start -->

The Log subject: a model's live llama-server output, at `/models/:id/log`,
opened from the loaded model's `▸ Log` line or its failed load's Show log.
The inspector widens to 60% of the window while it is open.

Header `‹ <name> · Log   Following · Paused   ⧉  ⋯  ×`: ‹ goes back to the
model, Following and Paused are a word switch (scrolling away from the end
pauses too and the switch says so; scrolling back follows again; Following
jumps to the end), ⧉ copies every line, ⋯ holds Clear view (the daemon keeps
the log), × closes the inspector. Each line follows its time since the
first line shown, `m:ss.mmm`, unless llama.cpp stamped it itself
(`0.00.046.817`): one time per line. Lines wrap at spaces only, and a
longer token scrolls. Colours are the ANSI colours llama.cpp wrote.

The port is the running server's, or the last the server registry heard
for the model: a stop or a crash names no port, so the registry keeps the
last one, and that server's log stays on the daemon and on screen, unless
the daemon has since given that port to another model (it keeps logs by
port), which is said instead. With no port, one sentence says why: after a
failed load's Show log, the daemon named no port for that start (a first
start that fails before `server_started`); else the model is not loaded and
this window heard of no earlier run.

Words with no Rust twin: "Log", "Following", "Paused", "Clear view", "Back
to <name>", "Copy the log", "Waiting for server output" and its line, "<name>
is not loaded, and this window has no earlier log of it. Its log shows from
its next load.", "<name>'s last log is gone: its port now serves another
model.", "The output of this start is not available: the daemon named no
port for it."

| File | Role |
|------|------|
| `LogInspector.tsx` | The subject: the port from the server registry (the last known after a stop or crash, unless another model now serves it), or the sentence for a port given away, a failed start with no port, or a model not loaded |
| `ConsoleLogPanel.tsx` | The header (`LogHeader`) and the log: ANSI parsing (Anser), the time before each line, following, copy and Clear view |
| `ConsoleLogPanel.css` | Terminal appearance and the ANSI colour classes |

<!-- module-docs:end -->
