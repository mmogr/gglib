# library

<!-- module-docs:start -->

The library place: every model, as groups of rows in one order (Downloading,
Loaded, On this Mac, the paired machine's), a model in one of them; one
search that filters them as you type and takes the text to Hugging Face on
Enter (`hf/`); a sort; and the inspector on the right while it has a
subject. Rows are options in a listbox per group, and picking
one changes its fill and nothing else.

Every word on a row is the daemon's: a loaded model's state is
`RESIDENCY_WORDS` for the state the proxy's live dashboard gives its
resident (Loaded while the stream says nothing of it), the paired
machine's reach is `REACH_WORDS` for the remote status's `reach`, and a
download is its row's own text. The queue is `services/downloadsRegistry`;
the library is `services/modelRegistry`; the loaded models are
`services/serverRegistry`, and what each is doing `services/dashboardRegistry`.

The inspector beside it is `inspector/Inspector`, whose subject the
address names; a Hugging Face repo picked in the group opens at
`/get/:owner/:name`, its summary handed to the address.

| File | Role |
|------|------|
| `LibraryPlace.tsx` | The layout route over `/library`, `/models/:id` (and its `/log`), `/far/:id` and `/get/:owner/:name`: the main column (the add-a-file row, the rows, the hint row and the Hugging Face group) and the inspector, wired with `?add=` (focus the search, add a file), a Hugging Face pick's address, and the chat a started server opens |
| `LibraryHeader.tsx` | "Library", `Sort ▾` (Name, Recently added, Size, Measured speed; Ascending, Descending), `+ ▾` (Find on Hugging Face, Add a GGUF file…, Add a tool server…), and the one search field, Enter included |
| `LibraryRows.tsx` | The groups in order, each a listbox, the paired machine's headed by its name and reach; the empty, failed and loading states |
| `Group.tsx` | A group of rows under its 12 px label, and the one line said in place of rows |
| `useResidency.ts` | The endpoint's dashboard as this window reads it (`useEndpointDashboard`: the one place its target is built, from the status's `base_url`, the port and the revealed key; the endpoint place and the loaded inspector read it too), and what each loaded model is doing in it, by model id |
| `ModelRow.tsx` | One model: its name, one line of facts, at most one accessory, and the actions a hover or focus shows (copy the Model ID, Unload) |
| `DownloadRow.tsx` | One download: its name, and a bar with the row's percentage and time left, or the word for what it is doing; × cancels it or takes it off the queue. `Downloads`, the Downloading group, drawn over the rows |
| `EngineBanner.tsx` | The line above the groups when llama.cpp is missing or older than the pinned release, a model running on it or not, with Install or Update run through the daemon's install stream, its refusal shown as it came, and the status read again once it completes |
| `engineInstall.ts` | The banner's state for as long as the window is open: "Not now", and the install it started with its progress and how it ended, so leaving the library loses neither |
| `AddFileRow.tsx` | Add a GGUF file: a typed path and Add (the desktop fills it from the native picker), the daemon's refusal under it |
| `groups.ts` | Pure: the groups, the search's matching (every word must match), the sorts, and a model's facts (the row's line and the inspector's summary) |
| `ModelListSkeleton.tsx` | Six still grey two-line rows before the library's first read |

| Directory | Contents |
|-----------|----------|
| `hf/` | The one search's Enter: the Hugging Face group, its search, and the suggestion for this machine |

<!-- module-docs:end -->
