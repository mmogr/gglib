# Sidebar

<!-- module-docs:start -->

The sidebar names what gglib is, on every screen: the Library, the Endpoint
with its address, the Devices; Chats under them; Settings and Diagnostics at
the foot. It replaces the header. The open row is read from the address and
every row navigates through `app/paths`, so the sidebar, the native menu and
a test move the app the same way.

Every word it shows is the daemon's: the endpoint's state word, reached
address and key rule come from `services/proxyRegistry` (the status the
daemon answers, re-read after each event), the loaded count from
`services/serverRegistry`, the devices' reach from `services/remoteRegistry`,
and the words themselves from `components/status/words.ts`. Nothing is
computed here.

Interim homes, each named with the PR that removes it: the endpoint line
opens today's proxy panel (`components/ProxyControl.tsx`, → N1); the devices
line opens today's remote panel (`components/RemoteControl.tsx`, → V1); Chats
opens the latest chat (→ C1); Settings opens the dialog over the place
(→ S1); Diagnostics opens the benchmarks (→ G1).

| File | Role |
|------|------|
| `Sidebar.tsx` | The column: the strip the traffic lights sit on, Library with its count, the endpoint and devices lines, Chats, the foot; `placeOf(pathname)` says which row the address belongs to |
| `EndpointLine.tsx` | `● Endpoint On`, the address with ⧉, and the second line (`secondLine`: how many loaded, nothing yet, only the pinned model, off); opens the proxy panel beside it |
| `DevicesLine.tsx` | `Devices` with the tunnel's state at the right (`devicesSaying`: the joined machine and its reach, sharing and how many paired, a code waiting); opens the remote panel beside it |
| `openLatestChat.ts` | What Chats does until the list lives here: the newest conversation with a model, handed to the chat route; none, or a list that cannot be read, goes to the library |
| `placement.ts` | Where a line's panel is drawn: beside the line, or across the window when the sidebar is a drawer |

<!-- module-docs:end -->
