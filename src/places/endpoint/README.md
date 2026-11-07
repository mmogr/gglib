# endpoint

<!-- module-docs:start -->

The endpoint place, `/endpoint`: the address and what can be done to it,
what is loaded and how, what it serves, how a client connects, who is
using it, what gglib did to recent requests, and the rest of the
dashboard as sentences; the inspector beside it while `?inspect=model:<id>`
names a model (`app/paths` `inspectorSubject`; × drops only `inspect`).

Every fact is the daemon's: the endpoint's state, address, key rule and pin
from `services/proxyRegistry`; the residents, their flag lines, the queue,
the second slot's headroom, the requests in flight and recorded, and what
gglib did to each (`Intervention.word`, the words `gglib proxy dashboard`
prints) from the one dashboard stream (`places/library/useResidency`
`useEndpointDashboard`); the recipes and the proof from
`GET /api/proxy/clients` and `POST /api/proxy/prove`, said in the CLI's
words (`readings/connect`, pinned by `contracts/connectClient.test.ts`);
paired devices and their descriptions from `services/remoteRegistry`. State
words are `components/status/words.ts`'s.

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
for a slot · N requests · 4s", "Room for one more, about 9.0 GiB", the row
⋯'s "Chat with it", "Change launch options…", "Pin the endpoint to it",
"Show log", "Unload"; "Serves", "Every model", "Only <model>"; "Apps on this
Mac · N requests now" / "· no requests now", "No app has used it yet.",
"Connect a client"; Recent's clock time and "812 tokens · 34 t/s" (the rate
is `readings/throughput`'s, which no Rust computes); Activity's "Prompt cache
71% reused", "N tool calls repaired", "N loop-guard trips", "N client
sampling fields ignored", "Nothing to report yet.", "Clear prompt cache",
"Live numbers ›", "All ›"; and ⋯'s "Live numbers", "Endpoint settings…".
Durations are `formatDuration`'s ("4s", "4m 10s"), where the CLI prints
"4s" and "4m 10s" too but from its own function.

| File | Role |
|------|------|
| `EndpointPlace.tsx` | The route: the title and ⋯ (Live numbers, Clear prompt cache, Endpoint settings…), the sections in order once the dashboard has spoken, the Model ID Connect a client and Test it share, the dashboard dialog, and the inspector beside it |
| `Header.tsx` | The address line: On with the address, Copy, the meaning, Test it, Turn off; Pinned with its model and Unpin; Key required with Show; Off with Turn on, its sentence and Start when gglib launches; Starting; the daemon's failure and the proof's sentence |
| `Loaded.tsx` | The resident rows (word or Reading % with its bar, the flag line, hover Unload and Chat, ⋯), the queue's lines, the room line, the empty sentence and Load a model… (the library until the palette) |
| `Serves.tsx` | Serves ▾: Every model, the loaded models, the library's other chat models; the pin chosen by its name |
| `UsingIt.tsx` | Requests in flight from apps on this machine, one line per joined device in the daemon's words, or the empty sentence with Connect a client |
| `Recent.tsx` | The last five requests, newest first: time, model, tokens and rate, what gglib did; All ›; not drawn when empty |
| `Activity.tsx` | ▸ Activity: the dashboard's other sections as sentences, Clear prompt cache, Live numbers › |
| `lines.ts` | Pure: the loaded rows, the queue's lines, the room line, the recent rows, the cache's reuse and the activity sentences, from one dashboard frame |
| `useEndpointActions.ts` | Turn on (no port), Turn off, serve one model or every model (stop, then start), and Test it, with the daemon's message for a failure |

<!-- module-docs:end -->
