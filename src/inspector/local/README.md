# local

<!-- module-docs:start -->

The inspector for a model on this machine. From the top: its name (⧉
copies the Model ID), ⋯ and ×; the library row's line of facts; the fit
sentence; Load and Chat (Chat and Unload once loaded, with its state word);
the Model ID to use it by; How it runs (How it ran once loaded); Details,
closed and remembered. The name and the actions stay in view while the
rest scrolls.

Load is one click and sends only the model's id. Chat on a model that is
not loaded loads it, then opens the chat on its port. Unload always asks the
daemon. With llama.cpp missing, Load is Install llama.cpp (the library
banner's install), then the load.

How it runs is the plan's arguments grouped by `LaunchArg.decision`, one
row per labelled decision (`rows.ts` holds the one label table), each
saying the plan's narration for that decision where it has one (the words
How it ran says), else its arguments, and whose choice it was and why on
hover; Templates is always drawn (Auto with no flag), Speculative too once
the person has set it, under the launch line
(`LaunchPlan::command_line` drawn token by token). Sampling and Reasoning
rows count the explanation's sources. Change turns a row into its control
and saves the one field of the update it changes; a row the person set
reads `set by you · Reset`. Overriding a token of the launch line, and the
per-launch options, are the Load with options sheet's (I2).

Words with no Rust twin, composed from Rust's fields here: the fit sentence
and its refusal, the Context row's note, the Sampling row's count, "set by
you", "Auto", "On", "Off", "Applies at next load.", "Didn't load:",
"Didn't unload:", "Drawing needs
stable-diffusion.cpp.".

| File | Role |
|------|------|
| `LocalInspector.tsx` | The composition: header and ⋯ menu, summary, state or fit sentence, the actions and their lines, the Model ID, How it runs, Details, and the Serve and verification dialogs until I2 and I3 |
| `HowItRuns.tsx` | How it runs (caption, launch line, a row per decision, the profile select ending Sampling) and, for a loaded model, How it ran from its launch's narration |
| `Row.tsx` | One row: label, value with why on hover, `set by you · Reset`, and Change, or the control in its place |
| `Change.tsx` | Each row's control (Context field, Templates and Speculative selects, Images and Parts file menus, the compact sampling form) and each row's Reset, saving one field of the update |
| `LaunchLine.tsx` | The launch line in mono, each argument a button opening a note with its reason and source |
| `commandLine.ts` | Pure: `commandLine`, `argText` and `shellToken`, the command line as `LaunchPlan::command_line` writes it |
| `rows.ts` | Pure: the decision labels, the rows of a plan, a row's value and hover, the caption, the fit sentence and Context note, the Sampling and Reasoning lines, and whether the person set them |
| `Details.tsx` | Details ▸: file, source, name and quant label (click to edit), architecture, trained context, tags, abilities, reasoning support, measured speed, raw GGUF metadata; open or closed remembered per viewer |
| `Tags.tsx` | The tags as chips with ×, and a field that adds one |
| `Abilities.tsx` | The four capability flags as boxes, with one footnote |
| `ReasoningSupport.tsx` | Whether the template reads `reasoning_effort`, re-checked while loaded, known after the first load otherwise |
| `ProjectorPicker.tsx` | The Images row's control: the projector over the daemon's choices plus None, saved on pick |
| `ComponentPickers.tsx` | The Parts row's control: one picker per role the image family needs, saved on pick |
| `useExplain.ts` | The model's explanation, read per model and profile and again after a change |
| `useLoad.ts` | Load, Chat (load first) and Unload, the llama.cpp install that a load waits on, and a failed load's words |
| `useLaunchFacts.ts` | A loaded model's `ServerInfo.launch`, read from the server list |
| `useModelDetail.ts` | The model's detail (tags, revision, metadata) and its tag edits |
| `useRetagModel.ts` | Re-detect or rebuild the detected tags: confirm on a rebuild, then toast and reload |
| `useSdStatus.ts` | The image runtime's status, read once for an image model (the Serve dialog reads it too) |

<!-- module-docs:end -->
