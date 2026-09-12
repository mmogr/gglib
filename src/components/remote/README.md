# remote

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-components-remote-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-components-remote-complexity.json)

<!-- module-docs:start -->

The `RemoteControl` popover's sections ([ADR 0012](../../../docs/adr/0012-the-remote-tunnel.md)): the two sides of the tunnel, and the roster of devices this machine serves. All of them read `remoteRegistry` and act through `getTransport()`; none knows it is inside a popover.

## Key Files

| File | Role |
|------|------|
| `ServeSection.tsx` | This machine as the desktop: enable (with the `/mcp` grant off by default), invite one more device, the status lines, disable |
| `DevicesSection.tsx` | Who holds a key, and the button that retires one. Rows come off the status this panel already reads rather than a fetch of their own, so the list cannot disagree with the tunnel state beside it |
| `PairingReveal.tsx` | The ticket and the code, shown once: the answer to `enable --invite` or `invite` is the only time the daemon hands that code out. Counts the code down and leaves at zero; the parent drops it the moment a device pairs, or once the daemon stops holding the code |
| `ConnectSection.tsx` | This machine as the laptop: the pairing string, the connected port as an `EndpointCopyBar`, the use-for-chat choice and the far machine's model name, disconnect, and the one-way door behind a confirm |

## What is deliberately not here

The connected port does not inject the key (decision 7), so the copy bar is shown with the reminder that a client supplies it. The status shows fingerprints and never a ticket, because the status is what a `GET` returns. There is no device-list fetch here either: the roster is a settings field the daemon reads to answer `status` anyway, so a second request would only be a second answer that can disagree.

<!-- module-docs:end -->
