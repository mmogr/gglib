// The browser-engine tests: tests/ts/browser/, run in real engines through
// Playwright (`npm run test:browser`). jsdom has no layout and leaves out
// whole APIs, so an engine's own answer to a DOM call is seen only in an
// engine: scrollIntoView returns a Promise in Chromium and in Edge, whose
// engine is the desktop app's webview on Windows, undefined in WebKit, and
// jsdom has none (tests/ts/browser/PickASize.test.tsx).
//
// A config of its own rather than a project in vite.config.ts, so a bare
// `vitest run` stays the jsdom suite and needs no browser installed.
//
// Chromium and WebKit by default. GGLIB_BROWSER=msedge runs the installed
// Microsoft Edge instead (Playwright's `msedge` channel), for the weekly run
// in .github/workflows/browser-edge.yml.
import { defineConfig } from 'vitest/config';
import react from '@vitejs/plugin-react';
import { playwright } from '@vitest/browser-playwright';

const instances =
  process.env.GGLIB_BROWSER === 'msedge'
    ? [{ browser: 'chromium' as const, name: 'msedge', provider: playwright({ launchOptions: { channel: 'msedge' } }) }]
    : [{ browser: 'chromium' as const }, { browser: 'webkit' as const }];

export default defineConfig({
  plugins: [react()],
  test: {
    include: ['tests/ts/browser/**/*.test.{ts,tsx}'],
    browser: {
      enabled: true,
      headless: true,
      provider: playwright(),
      instances,
      // A failure's message says what went wrong; a screenshot beside the
      // test would be a stray file in the tree.
      screenshotFailures: false,
    },
  },
});
