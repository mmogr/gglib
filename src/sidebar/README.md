# Sidebar

<!-- module-docs:start -->

The sidebar names what gglib is, on every screen: the Library, the Endpoint
with its address, the Devices, each a row with an icon, its state's shape as
a badge on the icon and its state's word at the right edge; the chats under them, scrolling while the
rest stays; Settings and Diagnostics pinned at the foot. It replaces the header. The open row is read from the address and
every row navigates through `app/paths`, so the sidebar, the native menu and
a test move the app the same way.

Every fact it shows is the daemon's: the endpoint's state word, reached
address and key rule come from `services/proxyRegistry` (the status the
daemon answers, re-read after each event), the loaded count from
`services/serverRegistry`, the endpoint dot's blink from the dashboard
stream's request count (`places/library/useResidency`), the devices' reach and roster from
`services/remoteRegistry`, the chats from `services/chatRegistry`, and the
state words themselves from
`components/status/words.ts`, and sharing from the daemon's lines
(`RemoteStatus.sharing`, "Sharing", and `RemoteStatus.paired_line`, "2 paired"). No fact is computed here; the
short sentences around a count ("2 loaded", "code 1:42") are this folder's
own until the places that own them say them.

The endpoint line opens the endpoint place (`places/endpoint`) and the
devices line the devices place (`places/devices`), and Settings the settings
place on General (`places/settings`), and Diagnostics the diagnostics
place on Live (`places/diagnostics`), each raised while it is open.

The chats' own words, not the daemon's: Chats, Filter chats, New chat,
"No chats yet.", "No matching chats", "The chats couldn't be read.",
"On <machine>", "Reply running" (the arc), "New reply" (an unseen reply,
said to a screen reader beside the bold), "More for <title>", Rename,
Delete…, Chat title, Loading chats.

| File | Role |
|------|------|
| `Sidebar.tsx` | The column: the strip atop (`TitleStrip`: the traffic lights' alone in the Mac app, "gglib" elsewhere), Library with its count, the endpoint and devices lines, the chats, the foot; `placeOf(pathname)` says which row the address belongs to |
| `PlaceRow.tsx` | One place's row: `[icon] name … word`, so every name shares one left edge and every word one right edge; the state's shape is a badge on the icon; detail lines sit under the name, inside the button or, holding a control, under it; Settings and Diagnostics keep the icon column empty |
| `ChatList.tsx` | "Chats ⌕ +" (a small section label, not a place) over the recent chats, title only, newest first, the open one marked; a running reply's arc, an unseen reply in bold; ⋯ Rename · Delete… (asked first); ⌕ a filter field; + an empty chat at once; the joined machine's own chats as an "On <machine>" group at the foot; "No chats yet." / "No matching chats"; a `role="status"` skeleton while read |
| `useConversationActivity.ts` | Running (a live agent run, from `GET /api/runs`, polled every 3 s while the sidebar is drawn) and New (a reply that ended while another chat was open); New marks are kept in this browser as conversation id → end time, read before each write and followed across tabs, and dropped only when a list the daemon just sent lacks the conversation and the mark predates asking for it; the far machine's runs and marks are its own (`gglib.chat.unread.far`), so its ids never meet this machine's |
| `EndpointLine.tsx` | `Endpoint … On` with the state's badge on the plug, the address with ⧉ (which copies and goes nowhere), and the second line (`secondLine`: how many loaded, nothing yet, the pinned model's name for a pinned run, click to turn on); the badge blinks once per served request (`components/status/useRequestBlink`); a click opens `/endpoint`, raised while it is open |
| `DevicesLine.tsx` | `Devices` with the tunnel's state at the right and its badge on the icon (`devicesSaying`: the joined machine's reach with its name under it, sharing in the daemon's words with the paired count under it, a code on offer as `code 1:42` counting down to its deadline, in plain text with no badge, since ▲ is for warnings); opens the Devices place |

<!-- module-docs:end -->
