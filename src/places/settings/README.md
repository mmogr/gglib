# settings

<!-- module-docs:start -->

The settings place, at `/settings/:section`: six sections on the left
(General, Endpoint, Models, Tools, Engines, About), the open one on the
right, `/settings` and a segment that names no section going to General.
Reached by the sidebar's foot, the native menu's Settings…, the endpoint's
⋯ Endpoint settings…, the library's Add a tool server…, the inspector's
Install for drawing and the devices' Change ›.

Every field saves as it changes: a checkbox or a choice when it moves, a box
when it is left or Enter is pressed (Escape puts the stored value back). A
write sends that one field under the binding's own name and nothing else,
and nothing for a value already stored (ST-1, ST-7, `useFieldSave`); it goes
through `SettingsContext`'s `save` with `quiet`, so the field says how it went
and no toast does. "Saved." with Undo stays for five seconds, and Undo writes
back what was stored, `null` included. A refused write is said under its
field in the daemon's words, and the box keeps what was typed. There is no
Save and no Cancel. The API key is held, not shown (ADR 0027): Show and Copy
reveal it each time; Replace asks first; Clear writes at once; both reveal
the key they replace first, for Undo, and write nothing when it cannot be
read. Numbers take their defaults and ranges from
`constants/settingsDefaults` (held to Rust by `contracts/settingsBounds.test.ts`):
the default in the box and as "Default: N" under it, announced with the
field (ST-2). The drawing reading is `readings/drawingValues`.

The loop guard is a checkbox: on writes `note`, off writes `off`; `refuse`
is the terminal's and shows as on with a line saying so. The image model
choice lists only models that draw, keeping a stored id that is not one.

Interim sections, each until the PR that replaces it:

- **Tools** hosts today's `McpServersPanel` and its add and edit dialog over
  one list (ST-8) until S2 (`feat(gui)/settings-tools`); "This device" is
  the chat's per-device `AgentLimitsSection`.
- **Engines** hosts today's `EngineCard`s until S3
  (`feat(gui)/settings-engines-about`), and the system checks
  (`DiagnosticsPanel`) until G1 (`feat(gui)/diagnostics-place`).
- **About** shows the build and today's Stop the daemon until S3.
- The loop guard's trips (`LoopGuardTripsPanel`) sit under Stop runaway
  loops until G1 moves them to Diagnostics › Live.

| File | Role |
|------|------|
| `SettingsPlace.tsx` | The route: the section list (the open one raised, `aria-current`), the section's name over its content in the one scrolling area, a turning mark until the settings have loaded, a segment that names none sent to General; reads the settings again on opening |
| `GeneralSection.tsx` | Start gglib at login, Start the endpoint when gglib launches, Keep running in the menu bar when the window closes (desktop app only) |
| `EndpointSection.tsx` | Port, the API key, Share on the local network, Bind address, Endpoint address, Allowed hosts (`hostsText`/`hostsFrom`), the prompt cache on disk and its budget, Model servers start at; What gglib does to requests: Repair broken tool calls, Stop runaway loops (with its trips), Cap sampling on agent runs, Let clients set sampling |
| `ModelsSection.tsx` | The models folder, Download staging folder, Download queue size, Context for models without their own, Model for gglib q, Image model for drawing (`imageChoices`), Sampling defaults, Profiles |
| `ToolsSection.tsx` | The tool servers (interim), Let MCP clients draw, Agent limits (Max tool rounds, Max stuck steps, This device), Title prompt with Reset |
| `EnginesSection.tsx` | An engine card for llama.cpp and for stable-diffusion.cpp, then the system checks (interim) |
| `AboutSection.tsx` | The build, and Stop the daemon asked first, then "Shutdown requested" (interim) |
| `useFieldSave.ts` | One field's write: `useUndoableWrite` (nothing for the value stored, Undo for `UNDO_MS`, the refusal kept), `useSettingSave` (`{ [key]: value }` through the provider), `sameValue` |
| `fields.tsx` | The fields that save themselves: `TextSetting` (a box, or a text area, with Reset where asked), `NumberSetting` (its spec's default, hint and range), `ToggleSetting` (its polarity and what it writes), `ChoiceSetting`; `useStored` reads a setting back under the name it writes |
| `SettingField.tsx` | A field's label, control, hint row (`settingDescriptionId` for `aria-describedby`), restart note and save line |
| `SaveLine.tsx` | Under a field: the refusal, or "Saved." with Undo |
| `SecretSetting.tsx` | The API key: Set with nothing stored; Show, Copy, Replace (asked first) and Clear with one held; Undo writing the key before back |
| `ModelsFolder.tsx` | The models folder: typed, or Change… with the native picker on the desktop, Reset to the default, where it came from, "exists · writable"; its own write (`setModelsDirectory`) with Undo |
| `SamplingDefaults.tsx` | One line of what is set, Change… opening the sampling form in place (the fields set and Add a value), Save writing `inferenceDefaults` alone |
| `InferenceProfiles.tsx` | The profiles: a row each with "in model picker", Edit and ⋯ Delete… (asked first), Add profile, Install starter profiles (the daemon's nine); the editor in a sheet; every change writes the whole list (`summarize` for a row's values) |
| `InferenceProfileEditor.tsx` | One profile's form, its fields unchanged (ST-5): name (checked as the server checks it), description, the sixteen numbers left blank to fall through, reasoning effort, Show in the model picker |
| `Disclosure.tsx` | A ▸ line opening what it holds in place, closed when the section opens |
| `notes.ts` | When a change takes effect, for the fields read when the endpoint turns on, when gglib starts, or when a model loads |
| `EngineCard.tsx` | One engine's card: what is installed and how, where it stands against its pin, install or update with progress, the CPU build's warning, the source build's command, a removal that waits for a running model (ST-11) |
| `pinStanding.ts` | Where an engine's install stands against its pinned release, in the card's words, and whether to offer the update or send a source build to the terminal |
| `DiagnosticsPanel.tsx` | `gglib config check-deps`, `paths` and `fast-downloads status` as one panel |
| `useDiagnostics.ts` | State for the system checks |
| `LoopGuardTripsPanel.tsx` | `gglib proxy trips`: per day and model, what the loop guard scanned and what it acted on |
| `useLoopGuardTrips.ts` | The trips' one read when the panel opens, and a reload |
| `BuildInfo.tsx` | The daemon's own build, read from `/api/version` |
| `LabelledValue.tsx` | A label and its value, for the read-only rows of the cards and checks |

## Words this folder says that Rust does not

The section names (General, Endpoint, Models, Tools, Engines, About); every
field's label and description; "Takes effect when the endpoint next turns
on.", "Takes effect when gglib next starts.", "Takes effect for the next
model loaded."; "Saved." / "Undo"; "Asking gglib" and "Couldn't read the
settings: <the daemon's words>"; "What gglib does to requests", "Agent
limits", "This device"; the loop guard's "Refusing, as … set it: a looping
request is answered with a 400."; the API key's "Set", "Replace", "Clear",
"Cancel", "The new key", "none", the Show/Hide/Copy labels, "Copied the API
key." and its failures, "Replace the API key?" with "Clients that send the
old key are refused once the endpoint next turns on.", and "The key in use
could not be read, so it was left as it is: <the daemon's words>"; the
models folder's "Change…", "Reset", its sources' sentences, "exists" /
"will be created", "writable" / "not writable", "Choose the models folder";
"None" in the model choices and "Model <id> (not an image model in this
library)"; "Sampling defaults", "None set", "Change…", "Save", "Cancel" and
the form's caption; "Profiles", its sentence, "No profiles yet. Add one, or
install the starter profiles.", "in model picker", "Edit", "More for
<name>", "Delete…" and "Delete the profile "<name>"?", "Add profile", "Edit
<name>"; the Title prompt's "Reset".

<!-- module-docs:end -->
