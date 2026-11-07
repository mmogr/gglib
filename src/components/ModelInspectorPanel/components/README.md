# components

<!-- module-docs:start -->

The old inspector's parts still in use, each until the PR that replaces it.

## Key Files

| File | Role | Removed by |
|------|------|------------|
| `ServeModal.tsx` | The Serve dialog: context override, custom port, Jinja mode, MTP settings, inference params; for an image model those give way to `ImageServeDetails`, and Start waits for its components and the image runtime. Opened by the inspector's Load with options… and Load and pin, `?serve=1` and the native menu's Serve | I2 |
| `ImageServeDetails.tsx` | What loading an image model means: its family, each linked component (a missing one named), and whether `sd-server` is installed, with the way to Settings when it is not | I2 |
| `JinjaModeField.tsx` | Off / On / Defer as three options, because a launch has three states and a checkbox held two | I2 |
| `contextPlaceholder.ts` | What an empty context box will actually get you, in the order the daemon's ladder resolves | I2 |
| `ModelMetadataGrid.tsx` | Read-only grid of a far model's detail: size, architecture, quantization with its bits per weight, context window, HF link, revision and times | I4 |
| `InfoRow.tsx` | One label/value row, the unit the metadata grid is built from | I4 |
| `MetadataSection.tsx` | Groups `InfoRow`s under a heading | I4 |

<!-- module-docs:end -->
