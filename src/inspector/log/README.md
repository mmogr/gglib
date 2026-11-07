# log

<!-- module-docs:start -->

The Log subject: a model's live llama-server output, at `/models/:id/log`,
opened from the loaded model's `▸ Log` line or its failed load's Show log.
The inspector widens to 60% of the window while it is open.

Header `‹ <name> · Log   Following · Paused   ⧉  ⋯  ×`: ‹ goes back to the
model, Following and Paused are a word switch (scrolling away from the end
pauses too; Following jumps back to it), ⧉ copies every line, ⋯ holds Clear
view (the daemon keeps the log), × closes the inspector. Each line follows
its time since the first line shown, `m:ss.mmm`; lines wrap at spaces only,
and a longer token scrolls. Colours are the ANSI colours llama.cpp wrote.

The port is the running server's, or the last the server registry heard
for the model: a stopped or crashed server's log stays on the daemon. With
neither, one sentence says the model has not run since gglib started.

Words with no Rust twin: "Log", "Following", "Paused", "Clear view", "Back
to <name>", "Copy the log", "Waiting for server output" and its line, "<name>
has not run since gglib started; its log starts when it loads."

| File | Role |
|------|------|
| `LogInspector.tsx` | The subject: the port from the server registry, or the sentence for a model that has not run |
| `ConsoleLogPanel.tsx` | The header (`LogHeader`) and the log: ANSI parsing (Anser), the time before each line, following, copy and Clear view |
| `ConsoleLogPanel.css` | Terminal appearance and the ANSI colour classes |

<!-- module-docs:end -->
