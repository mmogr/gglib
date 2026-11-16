# tests/ts/browser

Tests that run in real browser engines rather than jsdom: Chromium and WebKit
through Playwright, as Vitest's browser mode, from `vitest.browser.config.ts`.
jsdom has no layout and leaves out whole APIs, so what an engine itself
answers is seen only here. `scrollIntoView` is one: it returns a Promise in
Chromium and in Edge, whose engine is the desktop app's webview on Windows,
undefined in WebKit, and jsdom has none. An effect that returns its value
hands React a cleanup that is not a function, and React throws when it calls
it (A2 on [#1959](https://github.com/mmogr/gglib/issues/1959)).

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
