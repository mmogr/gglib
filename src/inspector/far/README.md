# far

<!-- module-docs:start -->

The inspector's far subject: the paired machine's model, at `/far/:id` (its
id there), read-only. From the top: its name (⧉ copies it) and ×; "on
<machine> · ● <reach>", the reach in `REACH_WORDS` for the remote status's
`reach`, as the library's group heads it; the summary its row carries
(quant, "sees images", its description); `[■ Load on <machine>] [Chat]`,
each only where that machine lists the action; "● Loaded on <machine>" once
it serves; ▸ Details.

While that machine is away the actions stay, disabled, and one line says
why; while its rows or the model could not be read, they are disabled and
one line says so with Retry, which reads both again. Load reads the model
again when it lands, which is how "Loaded on" appears and Load goes; a
Load's wait and failure belong to the model it was pressed for. Details
holds what that machine stores (size, architecture, quant and bits per
weight, trained context, repo ↗, revision with its full value on hover,
when it was downloaded; no path on its disk) and `gglib chat <id>
--remote` ⧉, since that id without the flag is this machine's model.

Words with no Rust twin: "on <machine>", "Load on <machine>", "<machine> is
away; Load and Chat wait until it is back.", "could not be read just now.",
"Retry", "Didn't load:", "From a terminal".

| File | Role |
|------|------|
| `FarInspector.tsx` | The subject: header, reach line, summary, the listed actions, the away and could-not-read lines, Load and its re-read |
| `FarDetails.tsx` | ▸ Details: the facts that machine exposes and the `--remote` command ⧉, open or closed as the local model's were last left |

<!-- module-docs:end -->
