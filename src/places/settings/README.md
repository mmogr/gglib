# settings

<!-- module-docs:start -->

The settings place, at `/settings/:section`: six sections on the left
(General, Endpoint, Models, Tools, Engines, About), the open one on the
right, `/settings` and a segment that names no section going to General.
Reached by the sidebar's foot, the native menu's Settings…, the endpoint's
⋯ Endpoint settings…, the library's Add a tool server…, the inspector's
Install for drawing and the devices' Change ›.

Every field saves as it changes: a checkbox or a choice when it moves, a box
when it is left or Enter is pressed, a text area when it is left (Escape puts
the stored value back in either). A field is held while its write is on its
way, so an edit cannot be judged against the value before it. A
write sends that one field under the binding's own name and nothing else,
and nothing for a value already stored (ST-1, ST-7, `useFieldSave`); it goes
through `SettingsContext`'s `save` with `quiet`, so the field says how it went
and no toast does. "Saved." with Undo stays for five seconds, and Undo writes
back what was stored, `null` included. A refused write is said under its
field in the daemon's words, and the box keeps what was typed. There is no
Save and no Cancel. A field the Rust reads only at a start says which start
(`notes.ts`, each note from the code that reads the setting); one with no
note is read where it is used. Opening the place reads the settings again,
and a read a save overtook is dropped (`SettingsContext`, TX-2). The API key is held, not shown (ADR 0027): Show and Copy
reveal it each time; Replace asks first; Clear writes at once; both reveal
the key they replace first, for Undo, and write nothing when it cannot be
read. Numbers take their defaults and ranges from
`constants/settingsDefaults` (held to Rust by `contracts/settingsBounds.test.ts`):
the default in the box and as "Default: N" under it, announced with the
field (ST-2). The drawing reading is `readings/drawingValues`.

The loop guard is a checkbox, on unless stored off. The image model
choice lists only models that draw, keeping a stored id that is not one.

Tools' tool servers are `tools/` (its own README): a row each and one
sheet to add and edit, over one list (ST-8). Agent limits are this
machine's, for every agent run on it: Max tool rounds, Max stuck steps,
Parallel tool calls and Tool timeout, the last two outranked by a chat's
own where it saved one. There are no per-device limits. A reasoning effort
and budget are Models › Sampling defaults'.


| File | Role |
|------|------|
| `SettingsPlace.tsx` | The route: the section list (the open one raised, `aria-current`), the section's name over its content in the one scrolling area, a turning mark until the settings have loaded, a segment that names none sent to General; reads the settings again on opening |
| `GeneralSection.tsx` | Start the endpoint when gglib launches; in the desktop app only, Start gglib at login and Keep running in the menu bar when the window closes |
| `EndpointSection.tsx` | Port, the API key, Bind address (the endpoint's), Allowed hosts (`hostsText`/`hostsFrom`), the prompt cache on disk and its budget, Model servers start at; What gglib does to requests: Repair broken tool calls, Stop repeated calls, Let clients set sampling |
| `ModelsSection.tsx` | The models folder, Download queue size, Context for models without their own, Model for gglib q, Image model for drawing (`imageChoices`), Sampling defaults, Profiles |
| `ToolsSection.tsx` | The tool servers (`tools/`, over the one `useMcpServers`), Let MCP clients draw, Agent limits (Max tool rounds, Max stuck steps, Parallel tool calls, Tool timeout (ms)), Title prompt with Reset, the "Learn more about MCP" footnote |
| `tools/` | The tool server rows and their add and edit sheet: its own README |
| `EnginesSection.tsx` | A row for llama.cpp and for stable-diffusion.cpp, then Faster downloads |
| `AboutSection.tsx` | The build, Documentation (the native Help menu's address), and Stop gglib… asked first, then held as Stopping gglib… until the window's service-not-running screen takes over (`app/ServiceDown`); a refusal in the daemon's words |
| `useFieldSave.ts` | One field's write: `useUndoableWrite` (nothing for the value stored, Undo for `UNDO_MS`, the refusal kept), `useSettingSave` (`{ [key]: value }` through the provider), `sameValue` |
| `fields.tsx` | The fields that save themselves: `TextSetting` (a box, or a text area, with Reset where asked), `NumberSetting` (its spec's default, hint and range), `ToggleSetting` (its polarity and what it writes), `ChoiceSetting`; `useStored` reads a setting back under the name it writes |
| `SettingField.tsx` | A field's label, control, hint row (`settingDescriptionId` for `aria-describedby`), restart note and save line |
| `SaveLine.tsx` | Under a field: the refusal, or "Saved." with Undo |
| `SecretSetting.tsx` | The API key: Set with nothing stored; Show, Copy, Replace (asked first) and Clear with one held; Undo writing the key before back |
| `ModelsFolder.tsx` | The models folder: typed, or Change… with the native picker on the desktop, Reset to the default, where it came from, its state (`MODELS_DIR_WORDS`: Writable, or Will be created or Not writable with ▲); its own write (`setModelsDirectory`) with Undo |
| `SamplingDefaults.tsx` | One line of what is set, Change… opening the sampling form in place (the fields set and Add a value), Save writing `inferenceDefaults` alone |
| `InferenceProfiles.tsx` | The profiles: a row each with "in model picker", Edit and ⋯ Delete… (asked first), Add profile, Install starter profiles (the daemon's nine); the editor in a sheet; every change writes the whole list (`summarize` for a row's values) |
| `InferenceProfileEditor.tsx` | One profile's form, its fields unchanged (ST-5): name (checked as the server checks it), description, the sixteen numbers left blank to fall through, reasoning effort, Show in the model picker |
| `Disclosure.tsx` | A ▸ line opening what it holds in place, closed when the section opens unless told open; never submits a form it sits in |
| `notes.ts` | When a change takes effect, for the fields read when the endpoint turns on, when gglib starts, or when the desktop app starts, and the key's own |
| `EngineRow.tsx` | One engine's row: the release, the download or acceleration, prebuilt or built from source, and its state in one line; Install, Update to the pin… (asked first), ⋯ Reinstall (over a download only) and Remove… (asked first), every one disabled while a model runs on it with which to stop (ST-11); the CPU build's warning where an install is offered, a source build's command, the install's progress and how it ended; ▸ Details |
| `EngineDetails.tsx` | An engine row's Details: the install record (release or built from, download or acceleration, date), the binary and what it reports, the model serving on it, where it stands against its pin (the daemon's `standing`, the words `gglib config <engine> status` prints); only what the status has |
| `FasterDownloads.tsx` | `gglib config fast-downloads`: Off with Set up, On with ⋯ Turn off, each read back from the setup status; with no Python, the command that builds it with ⧉ and no Set up; a failure in the daemon's words |
| `BuildInfo.tsx` | The daemon's own build, read from `/api/version`: "gglib <display>", and "Working tree modified since this build" when its fingerprint is dirty |
| `LabelledValue.tsx` | A label and its value, for an engine's Details |

## Words this folder says that Rust does not

The section names (General, Endpoint, Models, Tools, Engines, About); every
field's label and description; "Takes effect when the endpoint next turns
on.", "Takes effect when gglib next starts.", "Takes effect when the desktop
app next starts.", "A new key takes effect within a few seconds. A cleared
key stops being required when the endpoint next turns on."; "Saved." / "Undo"; "Asking gglib" and "Couldn't read the
settings: <the daemon's words>"; "What gglib does to requests", "Agent
limits", "Parallel tool calls", "Tool timeout (ms)", "How many tool calls an
agent run makes at once.", "How long one tool call may take before it is
stopped.", "Learn more about MCP"; the API key's "Set", "Replace", "Clear",
"Cancel", "The new key", "none", the Show/Hide/Copy labels, "Copied" beside
Copy (`components/CopyValue`) and the copy's failures, "Replace the API key?" with "Clients that send the
old key are refused within a few seconds.", and "The key in use
could not be read, so it was left as it is: <the daemon's words>"; the
models folder's "Change…", "Reset", its sources' sentences, "Choose the
models folder" (its state is `MODELS_DIR_WORDS`);
"None" in the model choices and "Model <id> (not an image model in this
library)"; "Sampling defaults", "None set", "Change…", "Save", "Cancel" and
the form's caption; "Profiles", its sentence, "No profiles yet. Add one, or
install the starter profiles.", "in model picker", "Edit", "More for
<name>", "Delete…" and "Delete the profile "<name>"?", "Add profile", "Edit
<name>"; the Title prompt's "Reset". Engines (an engine's state is
`ENGINE_WORDS`, the CLI's `Status:` words): "prebuilt" / "built from source", "Pinned to
<release>", "Install", "Update to <release>…" with "Update <product> to
<release>?" and its sentence, "More for <product>", "Reinstall",
"Remove…" with "Remove <product>?" and its sentence, "Stop <model>
first.", "<product> <release> is installed.", "Removed <paths>." / "Nothing
to remove: …", "Reading <product>…", "Details", the Details labels
(Install record, "none, installed outside gglib", Release / Built from,
Download / Acceleration, Downloaded / Built, Binary, Binary reports,
Serving), "The update is a source build, run from a terminal: <command>.", the
source build's two sentences;
"Faster downloads", its sentence (its state is `FAST_DOWNLOADS_WORDS`),
"Set up", "Setting up. This can take a minute.", "More for Faster
downloads", "Turn off", "It needs Python 3.10 or newer. Once that is
installed:", "Copy the command", "Copied" beside it. About: "gglib
<version>", "Working tree modified since this build", "Documentation",
"Stop gglib…", "Stopping gglib…", "Stop gglib?" with its sentence and
"Stop".

<!-- module-docs:end -->
