# tests/ts/browser

Tests that run in real browser engines rather than jsdom: Chromium and WebKit
through Playwright, as Vitest's browser mode, from `vitest.browser.config.ts`.
jsdom has no layout and leaves out whole APIs, so what an engine itself
answers is seen only here. `scrollIntoView` is one: it returns a Promise in
Chromium and in Edge, whose engine is the desktop app's webview on Windows,
undefined in WebKit, and jsdom has none. An effect that returns its value
hands React a cleanup that is not a function, and React throws when it calls
it (A2 on [#1959](https://github.com/mmogr/gglib/issues/1959)).

Layout is another: where a box is drawn is an engine's answer, and jsdom
gives every box the same empty rectangle. A test that reads layout imports
the app's stylesheet (`src/styles/tailwind.css`), which the Tailwind plugin
in `vitest.browser.config.ts` builds as the app's own build does, and first
checks that a class it depends on took effect, so it cannot pass on boxes
the app never draws.

```bash
npx playwright install chromium webkit   # once, and after Playwright moves
npm run test:browser
GGLIB_BROWSER=msedge npm run test:browser  # the installed Microsoft Edge
```

A bare `npx vitest run` is the jsdom suite and does not run these. CI's
`test-browser` job runs them in Chromium and WebKit on every change, and
`browser-edge.yml` runs them weekly in Edge on Windows. `tests/ts/setup.ts`
is not loaded here, so a test imports what it uses from `vitest` and asserts
with Vitest's own matchers.

| File | Pins |
|------|------|
| `PickASize.test.tsx` | Moving the size picker's selection and closing it raises nothing, with the engine's own `scrollIntoView` and with one that returns a Promise as Chromium and Edge do |
| `MenuLayout.test.tsx` | A `Menu`'s rows as the engine lays them out, in a submenu beside its item and in a menu under its own button: each row under the one before it with every line of its label inside it; a label that fits is one line; the menu is as wide as its rows, whatever a note says, and no wider than 24rem, a submenu 20rem, which beside the composer's `[+]` ends inside the chat at the app's smallest window; a label longer than that wraps, its row growing and each line starting where the first does; a name with nowhere to break is broken inside the menu |
