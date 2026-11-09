# endpoint

<!-- module-docs:start -->

The endpoint place, `/endpoint`: the address and what can be done to it,
what is loaded and how, what it serves, how a client connects, who is
using it, what gglib did to recent requests, and the rest of the
dashboard as sentences; the inspector beside it while `?inspect=model:<id>`
names a model (`app/paths` `inspectorSubject`; × drops only `inspect`).

Every fact is the daemon's: the endpoint's state, address, key rule and pin
from `services/proxyRegistry`; the residents, their flag lines, the queue,
the second slot's room in the daemon's sentence (`secondary_slot.detail`), the requests in flight and recorded, and what
gglib did to each (`Intervention.word`, the words `gglib proxy dashboard`
prints) from the one dashboard stream (`places/library/useResidency`
`useEndpointDashboard`); the recipes and the proof from
`GET /api/proxy/clients` and `POST /api/proxy/prove`, said in the CLI's
words (`readings/connect`, pinned by `contracts/connectClient.test.ts`);
paired devices and their descriptions from `services/remoteRegistry`. State
words are `components/status/words.ts`'s.

Load a model… opens a palette of this machine's models not loaded, each
with the fit sentence of its explanation (served on `plan.fit`, the
inspector's) and, when the launch would unload a model first, the CLI's
unload sentence (pinned by `contracts/unloadReason.test.ts`); the
joined machine's models follow as On <machine>, loadable there as the far
inspector allows. There is no route that explains many models, so each
row is read on its own (`GET /api/models/:id/explain`), the highlighted
row first, then the six rows the scroll has reached (rows are a fixed
52 px, so a fit line landing moves nothing), three at a time across opens.
The highlight is held by model, so rows coming and going do not move it. ⏎ loads with the model's id alone (IN-7);
⌥⏎ opens Load with options. No shortcut opens it: ⌘L is the browser's
address bar in `gglib web`.

Turn on names no port (EP-2); a failed start or stop is the daemon's
message as it said it (EP-1), so a port in use or llama.cpp missing is said
when the daemon says it, never detected here. Serving one model, or every
model again, restarts the endpoint: stop, then `start-pinned` or `start`,
since the daemon refuses a start while it runs. Unload asks the daemon
whatever this window believes (TX-12). Using it says "Apps on this Mac"
for every request in flight: the proxy records no client's identity, so
apps are not told apart. Not drawn here, for want of a fact:
"Shared on your network" (the status reaches a wildcard bind on loopback,
so no fact of the run says it is shared); the Copilot recipe's inline
"Serve only" (the recipe carries a hint, not a flag; Serves ▾ does it);
and while off, the models still in memory (the rows are the dashboard's,
which is idle while off).

Words of this place with no Rust twin: "Every model in your library answers
here by name.", "only <model>, other names are refused", "Apps can't reach
your models while it's off. Loaded models stay in memory.", "Start when
gglib launches", "Copy", "Test it", "Turn on", "Turn off", "Unpin", "Show",
"Hide", "Endpoint address copied", "Nothing loaded. An app loads a model by
asking for it by name, or load one now.", "Load a model…", "<model> waiting
for a slot · N requests · 4s", "Testing…", the row
⋯'s "Chat with it", "Change launch options…", "Pin the endpoint to it",
"Show log", "Unload"; "Serves", "Every model", "Only <model>"; "Apps on this
Mac · N requests now" / "· no requests now", "No app has used it yet.",
"Connect a client"; Recent's clock time and "812 tokens · 34 t/s" (the rate
is `readings/throughput`'s, which no Rust computes); Activity's "Prompt cache
71% reused", "N tool calls repaired", "N loop-guard trips", "N client
sampling fields ignored", "Nothing to report yet.", "Clear prompt cache",
"Live numbers ›", "All ›"; ⋯'s "Live numbers", "Endpoint settings…"; and the palette's "Load a
model" (its field), "fit unknown", "No model matches.", "No models on this
Mac.", "Every model on this Mac is loaded.", "On <machine>", "Load on
<machine>", "<machine> is away; Load waits until it is back.",
"<machine>'s models could not be read just now.", "<machine> does not offer
Load.", "Didn't load: <the daemon's words>" and "⏎ Load · ⌥⏎ Load with
options…".
The queue's wait is whole seconds, rounded down, as `gglib proxy dashboard`
prints it ("4s", "1m 5s"); the inspector's "loaded 4m 10s" is
`formatDuration`'s.

Connect a client's Model ID starts at the inspected model, else the pinned
one, else the last loaded, else the default model setting, else the
library's smallest chat model, so Test it never asks the endpoint to load
the biggest; pinned to a model that is not a chat model (an image model),
it is that model alone, with no picker. ⋯ Live numbers, Activity's Live numbers › and Recent's All ›
open Diagnostics › Live (`/diagnostics/live`). Clear prompt cache is offered
only while it can act (the endpoint up and its key revealed), and a row's
⋯ Pin waits while a pin change is on its way.

| File | Role |
|------|------|
| `EndpointPlace.tsx` | The route: the title and ⋯ (Live numbers, Clear prompt cache, Endpoint settings…), the sections in order once the dashboard has spoken, the Model ID Connect a client and Test it share, the inspector beside it `?connect=1` (the menu's Connect a Client…) scrolling to Connect a client once it is drawn, then leaving the address (at once while the endpoint is off) |
| `Header.tsx` | The address line: On with the address, Copy, the meaning, Test it, Turn off; Pinned with its model and Unpin; Key required with Show; Off with Turn on, its sentence and Start when gglib launches; Starting; the daemon's failure and the proof's sentence |
| `Loaded.tsx` | The resident rows (one Tab stop; ↑ ↓ inspect the next row as they move), and while the endpoint is off the running servers as Loaded, nothing when none (word or Reading % with its bar, the flag line, hover Unload and Chat, ⋯; Chat opens the model's latest chat, else an empty one, handed the slot's port through `app/openChat`), the queue's lines, the room line, the empty sentence and Load a model… (the palette) |
| `LoadPalette.tsx` | Load a model…: the filter, this machine's models not loaded with quant, size and fit, On <machine>, ↑ ↓ Home End, ⏎ loads with nothing typed, ⌥⏎ opens Load with options, Esc gives focus back |
| `paletteRows.ts` | Pure: the palette's rows (not loaded, matching the filter as the library's search does), a row's quant and size, its fit line from its explanation, and the paired machine's rows with why they cannot load there |
| `useFitReads.ts` | The palette's explain reads: the highlighted row first, then the rows in view, three at most in flight across opens, kept for one open, an answer to a closed open dropped |
| `Serves.tsx` | Serves ▾: Every model, the loaded models, the library's other chat models; the pin chosen by its model's id (`pinned_model_id`) |
| `UsingIt.tsx` | Requests in flight from apps on this machine, one line per joined device in the daemon's words, or the empty sentence with Connect a client |
| `Recent.tsx` | The last five requests, newest first: time, model, tokens and rate, what gglib did; All ›; not drawn when empty |
| `Activity.tsx` | ▸ Activity: the dashboard's other sections as sentences, Clear prompt cache, Live numbers › |
| `lines.ts` | Pure: the loaded rows (`residentRows`: the dashboard's, else the running servers once the list's read has landed, the tray's too), the queue's lines, the room line, the recent rows, the cache's reuse and the activity sentences, from one dashboard frame |
| `useClearCache.ts` | Clear prompt cache with the revealed key, its toast either way; also Diagnostics › Live's Clear |
| `useEndpointActions.ts` | Turn on (no port), Turn off, serve one model or every model (stop, then start), and Test it, with the daemon's message for a failure |

<!-- module-docs:end -->
