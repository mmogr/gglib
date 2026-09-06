# remote

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-components-remote-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-components-remote-complexity.json)

<!-- module-docs:start -->

The two halves of the `RemoteControl` popover ([ADR 0012](../../../docs/adr/0012-the-remote-tunnel.md)), one per side of the tunnel. Both read `remoteRegistry` and act through `getTransport()`; neither knows it is inside a popover.

## Key Files

| File | Role |
|------|------|
| `ServeSection.tsx` | This machine as the desktop: enable (with the `/mcp` grant off by default), the status lines, disable |
| `PairingReveal.tsx` | The ticket and the code, shown once: `enable`'s answer is the only time the daemon hands them out. Counts the code down and leaves at zero; the parent drops it the moment a device pairs |
| `ConnectSection.tsx` | This machine as the laptop: the pairing string, the connected port as an `EndpointCopyBar`, the use-for-chat choice, disconnect, and the one-way door behind a confirm |

## What is deliberately not here

The connected port does not inject the key (decision 7), so the copy bar is shown with the reminder that a client supplies it. The status shows fingerprints and never a ticket, because the status is what a `GET` returns.

<!-- module-docs:end -->
