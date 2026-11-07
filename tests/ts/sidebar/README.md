# tests/ts/sidebar

The sidebar, rendered inside the shell over the real routes
(`tests/ts/places/harness.tsx`, `renderShell`) or on its own under the
place providers, with the registries driven directly and the transport
stubbed.

| File | Pins |
|------|------|
| `sidebar.test.tsx` | The sidebar is on every screen and the open row is the address; Library counts the library and goes home; Settings opens the dialog over the place by replacing the entry; Diagnostics opens Live, marked current there; under 720 px the sidebar is a drawer behind a menu button, closed by a move; `placeOf` maps each address to its row |
| `chatList.test.tsx` | The chats: newest first, title only, the open one marked, opened by its address; a `role="status"` skeleton while read; a running reply's arc, an unseen reply's bold until opened, a mark for a chat no longer listed dropped; an unseen reply described as "New reply"; ⌕'s filter and "No matching chats", "No chats yet.", and a list that could not be read said as such; + opens an empty chat with no dialog; ⋯ Rename in place, Escape leaving it; ⋯ Delete… asks, and deleting the open chat opens an empty one on its model; the joined machine's own chats as "On desk" at the foot, not changed here, another machine joining listing its own chats and never the last one's, a machine that leaves taking its chats with it, and no group or far read when joined to nothing; the list scrolls and the foot stays; a reply's end reads the list again |
| `useConversationActivity.test.ts` | Running from `GET /api/runs` and New for a reply that ended while another chat was open; New marks kept in this browser, followed across tabs, dropped only on the daemon's word |
| `useConversationActivityFar.test.ts` | The far machine's runs and New marks apart from this machine's, under their own key |
| `endpointLine.test.tsx` | Off, On and Pinned in the daemon's words with the address without its scheme, the loaded count, the pin, the key glyph only when a request needs the key; between a start event and the daemon's answer the line says On and withholds the old address; ⧉ copies the whole address, says so, and goes nowhere; a click opens the endpoint place and marks the line as the place open; `shownAddress` and `secondLine` |
| `devicesLine.test.tsx` | What the line says (`devicesSaying`): nothing, the joined machine and its reach, sharing and how many paired, a code on offer as `code m:ss` over everything, with no mark, a turning mark before the first answer; the line, in the shell, is named by what it says, counts a code down to its deadline, and a click opens the Devices place and makes the line the open row |
