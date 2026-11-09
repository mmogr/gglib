# local

<!-- module-docs:start -->

The inspector for a model on this machine. From the top: its name (⧉
copies the Model ID), ⋯ and ×; the library row's line of facts; the fit
sentence; Load and Chat (Chat and Unload once loaded, with its state word);
the Model ID to use it by; How it runs (How it ran once loaded); ▸ Log with
the server's last line once loaded (it opens the Log subject); Details,
closed and remembered. A failed load's line offers Show log. × closes to
the library, or beside the endpoint to the endpoint place. The loaded
model's Context bar waits for a fact: the dashboard's `slots` are
llama.cpp's `/slots` for the primary resident alone, not keyed by model. The name and the actions stay in view while the
rest scrolls.

Load is one click and sends only the model's id; an image model's waits for
its missing parts, named under it. Chat on a model that is not loaded loads
it, then opens the chat on its port. Unload always asks the daemon. With
llama.cpp missing, Load is Install llama.cpp (the library banner's install),
then the load.

How it runs is the plan's arguments grouped by `LaunchArg.decision`, one
row per labelled decision (`rows.ts` holds the one label table), each
saying the plan's narration for that decision where it has one (the words
How it ran says), else its arguments, and whose choice it was and why on
hover; Templates is always drawn (Auto with no flag), Speculative too once
the person has set it, under the launch line
(`LaunchPlan::command_line` drawn token by token). Sampling and Reasoning
rows count the explanation's sources. Change turns a row into its control
and saves the one field of the update it changes; a row the person set
reads `set by you · Reset`. A token of the launch line whose decision the Load
with options sheet holds offers Change for this model…, which opens the
sheet on that field; a token the person chose is underlined. A per-token
override field and `+ flag` wait for a Rust surface that sets any flag.

⋯ Verify files and Check for updates (the second only for a model from
Hugging Face) start the check at once and open Details on Source, where
its line is: the file the daemon is hashing, with a bar; then the
daemon's verdict in its word (`VERDICT_WORDS`): `✓ Healthy`, `▲ Unhealthy
[Re-download them]`, or `▲ Unverifiable` when no file has a hash, with
every file's health (`FILE_HEALTH_WORDS`) in a list opened from the line;
for updates, up to date, or a newer revision with Download changed files
and Re-download at latest…;
a refusal in the daemon's words with Retry. Each repair and re-download asks
first, then runs on the download queue. The status is the model's, kept
across leaving it and never shown on another; a file check answered here
is the "files checked" time, since the daemon stores none.

Words with no Rust twin, composed from Rust's fields here: "loaded 4m 10s"
and the rate's "34 t/s" on a loaded model's status line, the fit sentence
and its refusal, the Context row's note, the Sampling row's count, "set by
you", "Auto", "On", "Off", "Applies at next load.", "Didn't load:" and
"Didn't unload:" (`utils/errors` `loadFailed`), "Copied" beside ⋯ after
⋯ › Copy path, "Drawing needs
stable-diffusion.cpp.", and the verification line's "Checking files…",
"Checking … N%", "Re-download it/them", "Checking for
updates…", "Up to date", "A newer revision exists (may be README only)",
"A newer revision exists: N files changed", "No revision is recorded for
this model, so there is nothing to compare.", "Download changed files",
"Re-download at latest…", "Sent to the download queue.", "Retry", and the confirms' words (kept from the verification
dialog). The verdict and per-file health words are
`OverallHealth::word` and `ShardHealth::word`, from `words.ts`.

| File | Role |
|------|------|
| `LocalInspector.tsx` | The composition: header and ⋯ menu, summary, state or fit sentence, the actions and their lines, the Model ID, How it runs, Details, and the Load with options sheet in its place when opened (by ⋯, a launch-line token, or `?serve=1`); ⋯ Verify files and Check for updates start the check and open Details on Source |
| `HowItRuns.tsx` | How it runs (caption, launch line, a row per decision, the profile select ending Sampling) and, for a loaded model, How it ran from its launch's narration |
| `Row.tsx` | One row: label, value with why on hover, `set by you · Reset`, and Change, or the control in its place |
| `Change.tsx` | Each row's control (Context field, Templates and Speculative selects, Images and Parts file menus, the compact sampling form) and each row's Reset, saving one field of the update |
| `LaunchLine.tsx` | The launch line in mono, each argument a button opening a note with its reason and source, and Change for this model… where the sheet holds its decision; a token the person chose underlined |
| `commandLine.ts` | Pure: `commandLine`, `argText` and `shellToken`, the command line as `LaunchPlan::command_line` writes it |
| `rows.ts` | Pure: the decision labels, the rows of a plan, a row's value and hover, the caption, the Context note, the Sampling and Reasoning lines, and whether the person set them |
| `Details.tsx` | Details ▸: file, source (with the verification line and the last file check), name and quant label (click to edit), architecture, trained context, tags, abilities, reasoning support, measured speed, the loaded server's llama-server port ("port 9001, internal"), raw GGUF metadata; open or closed remembered per viewer (the far model's Details share its fold, `Item` and `When`) |
| `LogLink.tsx` | `▸ Log` on a loaded model: the label opens `/models/:id/log`, the server's last line follows it in tertiary mono, cut with an ellipsis |
| `SourceStatus.tsx` | The line under Source a file check or update check writes: progress and bar, the daemon's verdict in its word and its action, the file list opened from it, a refusal with Retry |
| `useVerification.ts` | Each model's file check and update check, held per model outside the inspector: the `verification` stream's progress, the daemon's answers, a later run overtaking an earlier, and the confirmed repair, changed-files download and re-download |
| `Tags.tsx` | The tags as chips with ×, and a field that adds one |
| `Abilities.tsx` | The four capability flags as boxes, with one footnote |
| `ReasoningSupport.tsx` | Whether the template reads `reasoning_effort`, re-checked while loaded, known after the first load otherwise |
| `ProjectorPicker.tsx` | The Images row's control: the projector over the daemon's choices plus None, saved on pick |
| `ComponentPickers.tsx` | The Parts row's control: one picker per role the image family needs, saved on pick |
| `useExplain.ts` | The model's explanation, read per model and profile and again after a change |
| `useLoad.ts` | Load, Chat (load first) and Unload, the llama.cpp install that a load waits on, a failed load's words, and the sheet's load (the pin first when asked) |
| `useResident.ts` | A loaded model's status line from the dashboard: its residency word, its latest rate and how long it has been loaded ("Working · 34 t/s · loaded 4m 10s"); the launch state, else Loaded, without the dashboard |
| `useLaunchFacts.ts` | A loaded model's `ServerInfo.launch`, read from the server list |
| `useModelDetail.ts` | The model's detail (tags, revision, metadata) and its tag edits |
| `useRetagModel.ts` | Re-detect or rebuild the detected tags: confirm on a rebuild, then toast and reload |
| `useSdStatus.ts` | The image runtime's status, read for an image model and again when an install completes (its Load is off until stable-diffusion.cpp is there) |

<!-- module-docs:end -->
