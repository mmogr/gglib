# devices

<!-- module-docs:start -->

The Devices place, at `/devices` ([ADR 0012](../../../docs/adr/0012-the-remote-tunnel.md)):
this Mac shared with your devices, the devices that hold a key, and another
machine used from here. Reached by the sidebar's devices line.

Every fact is the remote status `services/remoteRegistry` holds, read again
when the place opens, and the pairing code on offer is the registry's too
(`services/remoteOffer.ts`, DV-1), so leaving the page loses nothing and the
sidebar counts it down. The reach words are `REACH_WORDS` and their marks
`REACH_MARK`; a device row's description is the daemon's
(`crates/gglib-app-services/src/remote/device_line.rs`); the command that
prints the paired machine's key is `REVEAL_COMMAND`, since that key is
terminal-only. The paired machine is shown by its name (`pairedName`), never
by its fingerprint.

| File | Role |
|------|------|
| `DevicesPlace.tsx` | The route: "Devices" and "End-to-end encrypted. No account." as wide as the column, the three blocks under it in the one scrolling area, a turning mark before the first status |
| `ShareBlock.tsx` | This Mac: Share with my devices (the key confirm only while the endpoint does not require a key; a code only while no device has joined), "Finding a relay…", Shared with your devices, Invite a device (dead while a code is live), Stop sharing (the offer dropped first), the fifteen-second read while a terminal's code is live |
| `PairingSheet.tsx` | The code on offer, under This Mac whenever the registry holds one: the QR for ggchat (the string uppercased, level M, quiet zone, black on white), the code in 3+3, the join string, the time left from the code's deadline; "<label> joined." then closed, "Expired." with New code |
| `PairedList.tsx` | Paired devices: each row's name (the id beside a label two rows share) and the daemon's description, no reach dot (the wire has none per device); ⋯ Forget… asked first, the list read again before anything is said; the tool servers shared with them while shared, Change › to Settings › Tools; the fingerprint and peers on one mono line |
| `JoinBlock.tsx` | Use another machine: Join with a string ("Reaching it…" in place, a refusal as it came), Reconnect only with a pairing and key remembered; joined, the machine's name, reach word and path (Away since when), its address ⧉, the command that prints its key ⧉, where its models are, Disconnect, ⋯ Stop <name>'s gglib… asked first; after a disconnect, the pairing remembered with Reconnect |
| `devices.ts` | Pure: the countdown as m:ss, the code as 3+3, the seconds left, the away time as HH:MM, the QR payload |
| `pageActions.ts` | The page's actions counted, so an error a block shows goes when its field changes or any other action on the page runs (`useBlockError`) |
| `useSecondsLeft.ts` | The seconds a code has left, ticking once a second from its own deadline; the sidebar's line reads it too |

## Words this folder says that Rust does not

"End-to-end encrypted. No account.", "This Mac", "Let your laptop and phone
use this Mac's models.", "Share with my devices", "Clients on this Mac will
need an API key from now on." / "Turn on", "Finding a relay…", "Shared with
your devices", "Invite a device", "Offering…", "Stop sharing", the sentence
for a switch on with nothing bound, "Scan with ggchat", "Code", "On a
computer:", "This code pairs <id>.", "Expires in", "<label> joined." / "A
device joined.", "Expired.", "New code", "Paired devices", "No devices yet.
Invite one above.", "Forget…" and its confirm, the forget's notices (forgot, held no such
device, the list not read again, could not forget),
"Tool servers shared with them:" / "none" / "Change ›", "Fingerprint" /
"peer(s)", "Use another machine", "Paste a pairing string", "Join",
"Reaching it…", "Last joined <name>", "Reconnect", the no-key line, "The
pairing is remembered.", "Connected. Reading which machine…", "since",
"reconnects on its own", "Clients need this device's key:", "<name>'s models
are in your Library.", "Disconnect", "Stop <name>'s gglib…" and its confirm,
"More for <name>", the ⧉ labels ("Copy the code", "Copy the pairing
string", "Copy the address", "Copy the command that prints the key"),
"Copied" beside a ⧉ (`components/CopyValue`), "Asking gglib" (the mark before the first status). The reach
words themselves are `REACH_WORDS`.

<!-- module-docs:end -->
