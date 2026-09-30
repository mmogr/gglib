# pages

Tests that render a whole page component and assert on which screen comes up.

Everything else under `tests/ts/` tests a unit — a hook, a service, a leaf component — and that is usually the right size. It is the wrong size for one class of defect: the page decides what to show, and a hook can be entirely correct while the page never calls it. That is the shape the remote-chat defect had. `useChatSession` would have opened a session for the far machine on request, and there was nothing to make the request and nothing rendering the result, so the chat screen was unreachable on a laptop with no local models.

So these tests drive the page the way a user does — click the control, assert the screen — and each one is checked by deleting the production line it is supposed to be pinning and watching it go red. A page test that survives its own subject being removed is worse than none, because it reads as coverage.

| File | Renders |
|------|---------|
| `ModelControlCenterPage.test.tsx` | The MCC's choice of screen: that the Remote panel's request opens a chat against the far machine with nothing served locally, and that closing it stops no server here; that a model already running here opens chat from the inspector's Open chat; that the composer's switch to a model not running serves it and remounts the chat on it with the same conversation |
| `ChatPageList.test.tsx` | `ChatPage`'s conversation list: visible by default, folded from the rail and remembered, unfolded to search; the view switcher and Close stay with the notebook; Running and New marks on the rows and in the rail button's name, New cleared by showing the conversation |
| `ChatPageNotebook.test.tsx` | `ChatPage` as a notebook: a saved reply's margin shows the figures it has and none it lacks, reads who → body → how it was made, opens its detail; a reply arriving shows its prompt progress; a saved reply is named by its model and says how it was made in the mock-up's order; a turn of only a tool call or only reasoning shows it rather than an empty body; the composer names the model and quantisation, as text or as a picker, and its tools popout grows rightwards from the margin |
| `chatPageHarness.tsx` | Not a test: the transport stub, providers and jsdom patches the chat page tests share |
| `ChatPageModelPicker.test.tsx` | The composer's model picker: running servers, then models that are not; a choice handed up with the open conversation; a page given a conversation opens on it |
| `ChatPageRemote.test.tsx` | `ChatPage` in remote mode: no Console tab, no capability probe for a model this machine does not have, and no read-only claim from a local server registry that knows nothing about the far one |

## Mocking, and what must not be mocked

`ChatPage` is stubbed in `ModelControlCenterPage.test.tsx` — it is lazily imported and pulls in the whole assistant-ui runtime, and what is under test there is which screen the page picks and what it hands it, both of which the stub shows. `ChatPageRemote.test.tsx` mounts the real one, so the two files together still cover the tree.

What is never mocked is the path from the control to the screen. The registry, the effect that reads it and the page that renders the result are all real, because that path is the whole subject.

Two jsdom gaps are patched locally rather than in `tests/ts/setup.ts`, since only this directory mounts the trees that need them: `ResizeObserver`, which assistant-ui's composer measures itself with, and a probe component for the toast queue — `ToastProvider` holds toasts but renders none of them (the container that does is mounted by `App`), so without the probe "no toast was raised" passes whatever happened.
