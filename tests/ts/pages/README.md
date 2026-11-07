# pages

Tests that render a whole page component and assert on which screen comes up.

Everything else under `tests/ts/` tests a unit — a hook, a service, a leaf component — and that is usually the right size. It is the wrong size for one class of defect: the page decides what to show, and a hook can be entirely correct while the page never calls it. A chat with the paired machine's model is that shape: `app/ChatRoute` opens it from its address, and only the page puts the far row, its inspector and the way to that address in reach of a laptop with no local models.

So these tests drive the page the way a user does — click the control, assert the screen — and each one is checked by deleting the production line it is supposed to be pinning and watching it go red. A page test that survives its own subject being removed is worse than none, because it reads as coverage.

| File | Renders |
|------|---------|
| `App.test.tsx` | The app shell past setup, its pages and services stubbed: a file dropped where nothing takes it is refused, so the desktop window cannot open it in place of the app, and a drag of anything else is left alone |

The chat's tests moved with it to `tests/ts/places/chat/`.

## Mocking, and what must not be mocked

`ChatPlace` is stubbed in `tests/ts/places/libraryChat.test.tsx` — it is lazily imported and pulls in the whole assistant-ui runtime, and what is under test there is which screen the page picks and what it hands it, both of which the stub shows. `tests/ts/places/chat/` mounts the real one, so the two together still cover the tree.

What is never mocked is the path from the control to the screen. The registry, the hooks that read it and the page that renders the result are all real, because that path is the whole subject.

Two jsdom gaps are patched locally rather than in `tests/ts/setup.ts`, since only this directory mounts the trees that need them: `ResizeObserver`, which assistant-ui's composer measures itself with, and a probe component for the toast queue — `ToastProvider` holds toasts but renders none of them (the container that does is mounted by `App`), so without the probe "no toast was raised" passes whatever happened.
