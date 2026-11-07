# hooks

<!-- module-docs:start -->

The old inspector's hooks still in use, each until the PR that replaces it.

## Key Files

| File | Role | Removed by |
|------|------|------------|
| `useServeModal.ts` | The Serve dialog's open state and its option values | I2 |
| `useServerActions.ts` | The Serve dialog's start: `serveModel()` (and the pin first, when asked) with what its fields hold | I2 |
| `useSamplingExplanation.ts` | The sampling half of a model's explanation, for the Serve dialog's captions | I2 |
| `useHfDownload.ts` | Queues a Hugging Face preview's download; the button is off when the queue snapshot says `full` | I4 |

<!-- module-docs:end -->
