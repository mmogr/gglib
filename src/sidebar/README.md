# Sidebar

<!-- module-docs:start -->

The sidebar names what gglib is, on every screen: the Library, the Endpoint
with its address, the Devices; Chats under them; Settings and Diagnostics at
the foot. It replaces the header. The open row is read from the address and
every row navigates through `app/paths`, so the sidebar, the native menu and
a test move the app the same way.

Every fact it shows is the daemon's: the endpoint's state word, reached
address and key rule come from `services/proxyRegistry` (the status the
daemon answers, re-read after each event), the loaded count from
`services/serverRegistry`, the devices' reach and roster from
`services/remoteRegistry`, and the state words themselves from
`components/status/words.ts`. No fact is computed here; the short sentences
around a count ("2 loaded", "Sharing · 2 paired", "Code waiting") are this
folder's own until the places that own them say them (V1).

The endpoint line opens the endpoint place (`places/endpoint`). Interim
homes, each named with the PR that removes it: the devices
line opens today's remote panel (`components/RemoteControl.tsx`, → V1); Chats
opens the latest chat (→ C1); Settings opens the dialog over the place
(→ S1); Diagnostics opens the benchmarks (→ G1).

| File | Role |
|------|------|
| `Sidebar.tsx` | The column: the strip the traffic lights sit on, Library with its count, the endpoint and devices lines, Chats, the foot; `placeOf(pathname)` says which row the address belongs to |
| `EndpointLine.tsx` | `● Endpoint On`, the address with ⧉ (which copies and goes nowhere), and the second line (`secondLine`: how many loaded, nothing yet, only the pinned model, off); a click opens `/endpoint`, raised while it is open |
| `DevicesLine.tsx` | `Devices` with the tunnel's state at the right (`devicesSaying`: the joined machine and its reach, sharing and how many paired, a code waiting); opens the remote panel beside it |
| `openLatestChat.ts` | What Chats does until the list lives here: the newest conversation with a model, handed to the chat route; none, or a list that cannot be read, goes to the library |
| `placement.ts` | Where a line's panel is drawn: beside the line, or across the window when the sidebar is a drawer |

<!-- module-docs:end -->
