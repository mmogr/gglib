# Tray

<!-- module-docs:start -->

The popover behind the system tray icon: a 360×480 window of its own
(`tray.html` → `tray-main.tsx`, with no providers and no router, TM-9).
"gglib", then three blocks: the Endpoint, Loaded and Devices, which scroll
inside the panel; the footer under them holds Turn on or Turn off, the
panel's one ink button, and Open gglib.

Every fact is the daemon's, read through the registries the main window
reads, whose bridges the panel starts itself and keeps for its life, so
there is one event stream however often the daemon comes and goes (TX-1):
the
endpoint's state word, reached address, key rule and pin from
`services/proxyRegistry`; the residents and their words from the one
dashboard stream (`places/library/useResidency` `useEndpointDashboard`,
rows by `places/endpoint/lines` `residentRows`), or the running servers from
`services/serverRegistry` while the stream is idle (the same function), once the list's read
has landed (`serverEvents` `useServerListHeard`); the joined machine, its
reach and the roster from `services/remoteRegistry`. State words are
`components/status/words.ts`'s. Turn on and Turn off are the endpoint
place's (`places/endpoint/useEndpointActions`: Turn on names no port,
EP-2; a failure is the daemon's message, EP-1), and × is
`services/server/safeActions` `safeStopServer`, which asks the daemon
whatever this window believes (TX-12).

No daemon is not a stopped endpoint (TM-8): while the daemon does not
answer (`hooks/useDaemonReachable`, its `/health` polled), the panel says so in one
sentence. In the desktop app Start gglib starts it as the tray menu does
(`start_gglib`), held as "Starting gglib…" until the poll hears it, and a
refusal is said under the sentence in the app's words; elsewhere it is
drawn disabled. The registries let go of what the daemon said when it
goes (`hooks/useDaemonRecovery`, which the main window's gate shares), and each is read again when the poll hears it (a list read replaces
the running set, so a model that died with the old daemon is not drawn),
and the event stream connects at once instead of sleeping out a reconnect
wait that grew while the daemon was gone. A list read that failed, or that an
event overtook, is tried again each poll until one lands.
Before the first answer, and until the proxy's read has landed, the panel
claims neither, and the footer has no Turn on or Turn off.
Everything here is HTTP except Open gglib, the panel's one window action:
`services/platform` `showMainWindow`, the Tauri command
`show_main_window`, allowlisted in `scripts/check-frontend-ipc.sh`; it is
drawn only in the desktop app. A failure is said under its block, cut to
fit with the whole message in its title.

Words of the panel with no Rust twin: "gglib", "Endpoint", "On · only
<model>", "Off. Clients can't connect.", "Key required · Copy key",
"Copied", "Loaded", "Nothing loaded. A model loads on its first request.",
"Nothing loaded." (while off),
"Unload <model>" (×'s name), "Devices", "Sharing", "On · N paired", "Off" (sharing), "gglib
isn't running, so nothing can be served." (with " Start it from the tray
menu." where there is no Start), "Start gglib", "Starting gglib…", "Turn on", "Turn off", "Open gglib", "Copy the endpoint
address" (⧉'s name, the sidebar's), "Failed" and "No further detail was
reported.".

| File | Role |
|------|------|
| `TrayPanel.tsx` | The panel: "gglib", the three blocks while the daemon answers, the not-running sentence while it does not, the footer (Turn on or Turn off, Start gglib, Open gglib), and the bridges it starts |
| `Endpoint.tsx` | `Endpoint ● On` (`endpointSaying`: Starting and Connecting as ◌, Pinned as "On · only <model>"), the address with ⧉ and a one-second Copied, Key required with Copy key, the dimmed Off line, and a failed turn on or off; the ● blinks once per served request (`components/status/useRequestBlink`) |
| `Loaded.tsx` | One row per loaded model (`trayLoadedRows`: the dashboard's, else the running servers as Loaded) with its word and ×; the empty sentence once known; a failed unload |
| `Devices.tsx` | The joined machine with its reach while there is one, and Sharing On · N paired or Off (`devicesLines`); not drawn before the first status |
| `Failure.tsx` | A failure under its block: ✕, the daemon's words cut to fit (`shownFailure`), never empty |

<!-- module-docs:end -->
