# tests/ts/sidebar

The sidebar, rendered inside the shell over the real routes
(`tests/ts/places/harness.tsx`, `renderShell`) or on its own under the
place providers, with the registries driven directly and the transport
stubbed.

| File | Pins |
|------|------|
| `sidebar.test.tsx` | The sidebar is on every screen and the open row is the address; Library counts the library and goes home; Chats opens the newest chat that has a model, handed to the chat route, and stays in the library with none or an unreadable list; Settings opens the dialog over the place by replacing the entry; Diagnostics opens the benchmarks; under 720 px the sidebar is a drawer behind a menu button, closed by a move; `placeOf` maps each address to its row |
| `endpointLine.test.tsx` | Off, On and Pinned in the daemon's words with the address without its scheme, the loaded count, the pin, the key glyph only when a request needs the key; between a start event and the daemon's answer the line says On and withholds the old address; ⧉ copies the whole address, says so, and opens nothing; a click opens today's proxy panel and a second closes it; `shownAddress` and `secondLine` |
| `devicesLine.test.tsx` | What the line says (`devicesSaying`): nothing, the joined machine and its reach, sharing and how many paired, a code waiting over everything, a turning mark before the first answer; the line is named by what it says and carries the waiting code as its title; a click opens today's remote panel and a second closes it |
