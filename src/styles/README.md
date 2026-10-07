# Styling & UI Architecture Contracts

<!-- module-docs:start -->

The contracts for gglib's Tailwind-first UI: where a design token lives, how a
component is styled, where platform code may sit, and the visual language every
screen implements. They hold for the desktop app and the web UI alike, which
render the same components.

| File | Role |
|------|------|
| `tailwind.css` | Tailwind v4 configuration: the fonts, the `@theme inline` bridge to the tokens, the keyframes and the base layer |
| `base/` | The design tokens (`variables.css`) and the highlight.js theme (`hljs.css`), described in [its README](base/README.md) |

---

## Design tokens

**CSS variables in [`variables.css`](base/variables.css) are the canonical design token source.**

- All design tokens (colors, spacing, typography, shadows, etc.) are defined as CSS variables in `:root`
- Tailwind consumes these tokens via the `@theme inline` block in [`tailwind.css`](tailwind.css)
- No parallel token systems—CSS variables are the single source of truth
- Token changes propagate automatically to both Tailwind utilities and vanilla CSS

Tokens are layered: a foundation value, then aliases named for their purpose.

```css
/* Foundation tokens (primitives) */
--color-primary: #f08c26;
--spacing-base: 1rem;

/* Semantic aliases (purpose-based) */
--color-background-secondary: var(--color-background-elevated);
--color-text-primary: var(--color-text);

/* Usage in components */
.button {
  background: var(--color-primary);     /* ✅ Vanilla CSS */
}
<div className="bg-primary">            {/* ✅ Tailwind utility */}
```

### Component color rule

> **No raw `rgba()` or `#hex` color values in component files.**
>
> All color references must use one of:
> - A Tailwind semantic utility class (e.g. `bg-danger-subtle`, `text-success`, `border-primary-border`)
> - A CSS variable reference (e.g. `var(--color-danger-subtle)`) — only when a Tailwind utility is unavailable

Inline arbitrary values like `bg-[rgba(239,68,68,0.15)]` or `text-[#ef4444]` are **banned**. Add tokens to `variables.css` instead. Nothing checks this rule ([Enforcement](#enforcement) lists what is checked), and [`base/README.md`](base/README.md) names its one standing exception.

---

## Tailwind first

**Tailwind is the default for layout and component composition**: layout, spacing, hover and focus states, responsive rules and token colors are utility classes in the TSX.

```tsx
<button className="flex items-center gap-2 px-4 py-2 bg-primary hover:bg-primary-hover rounded-md">
  <Icon icon={Plus} />
  Add Item
</button>
```

A stylesheet is for styling that does not map to utilities, such as a complex
`@keyframes` animation. It is not for layout, for a simple hover or focus
state, or for a color, spacing or type value a token already holds. Three
exist outside `styles/`, each beside the component that imports it:
`ConsoleLogPanel.css`, `InferenceParametersForm.css` and `RangeSlider.css`.
The same rule decides when a CSS module is allowed; the tree has none
(`find src -name '*.module.css'` prints nothing).

A new primitive replaces what it supersedes in one change: every usage moves to
it and the old asset is deleted, with no period in which both exist.

A file holds one responsibility. The file-size check covers every `.ts`,
`.tsx` and `.css` file under `src/` outside `src/types/generated/`:
CONTRIBUTING's [File size](../../CONTRIBUTING.md#file-size) says what its
budget is for, and its [UI Conventions](../../CONTRIBUTING.md#ui-conventions)
how to split a component that has taken on a second job.

---

## Platform parity

**UI must render identically in the Tauri desktop app and the web UI. Shared UI components must be platform-agnostic.**

✅ **Allowed in shared UI** (`src/components`, `src/pages`):
- React components, hooks, contexts
- Styling (Tailwind, stylesheets, CSS variables)
- Imports from `services/platform`
- Props for injecting platform-specific functionality

❌ **Not allowed in shared UI:**
- Direct imports from `@tauri-apps/api` or `@tauri-apps/plugin-*`
- Reading the Tauri bridge (`window.__TAURI_INTERNALS__`): ask `isDesktop()` from `services/platform` instead
- Platform-specific business logic

Platform-specific code belongs in `src/services/platform/`, and
[its README](../services/platform/README.md) lists the files there.

### TRANSPORT_EXCEPTION marker

A `// TRANSPORT_EXCEPTION:` comment marks a file where platform-specific
behaviour is unavoidable, so that those files are enumerable.
`scripts/check_transport_branching.sh` lists every file outside
`services/transport/` that reads the Tauri bridge and warns, without failing,
on one that carries no marker.

Note what the marker is *not* for. The transport itself does not branch: the
GUI talks HTTP to the daemon in both builds (see the header of
`src/services/serverEvents.ts`). A platform check inside
`src/services/clients/` fails that script, and one around a data path anywhere
else is not an example to follow. What legitimately carries the marker is
genuine OS integration: a native file dialog, a menu, opening a URL in the
system browser.

### Checking parity by hand

1. **Run the desktop app**: `npm run tauri:dev`
2. **Run the web UI**: `cargo run --package gglib-cli -- daemon run`, and `npm run dev` beside it
3. **Compare** buttons (hover, active, disabled), modals (open, close, backdrop click), form inputs (focus, validation, error states), layout on resize and icon rendering, side by side

---

## Tailwind v4 configuration

Tailwind v4 is configured in CSS: there is no `tailwind.config.js`, and
[`tailwind.css`](tailwind.css) is the configuration. In outline:

```css
@import "tailwindcss";

/* Use @theme inline to reference :root CSS variables */
@theme inline {
  --color-primary: var(--color-primary);
  --color-background: var(--color-background);
  --spacing-base: var(--spacing-base);
  /* ... all design tokens ... */
}

@layer base {
  :root {
    color-scheme: dark;
  }
}
```

`@theme inline` makes each utility reference the variable `variables.css`
already defines, so Tailwind generates `bg-primary` without a second set of
variables, and vanilla CSS goes on using `var(--color-primary)`.

```tsx
// ✅ Tailwind utility classes
<div className="bg-primary text-text border-border" />

// ✅ Arbitrary values with CSS variables
<div className="bg-[var(--color-primary-hover)]" />

// ✅ Vanilla CSS in a component's stylesheet
.myClass {
  background: var(--color-primary);
}
```

---

## File organization

```
src/
├── components/
│   ├── ui/                    # Controls: Button, Icon, Modal, Input, Tabs, …
│   ├── primitives/            # Layout and readouts: Stack, Row, Readout, …
│   ├── AddModel.tsx           # Feature components
│   └── Header.tsx
├── pages/                     # Route/page components
├── contexts/                  # React contexts
├── hooks/                     # Custom hooks
├── services/
│   ├── platform/              # OS integration
│   ├── clients/               # Backend clients with request logic of their own
│   ├── transport/             # HTTP and SSE to the daemon
│   └── …
├── styles/                    # This directory
├── types/                     # TypeScript types
└── constants/, utils/         # Shared constants and helpers
```

```typescript
// React
import { useState, useEffect } from 'react';

// UI primitives
import { Button } from './ui/Button';
import { Icon } from './ui/Icon';

// Icons
import { Plus, Check, X } from 'lucide-react';

// Platform code, through the barrel
import { isDesktop, pickGgufFile } from '../services/platform';
```

---

## Enforcement

What `eslint.config.js` actually enforces, all as errors:

- [x] No direct `@tauri-apps/*` imports in `src/components` or `src/pages`
      (the platform-boundary block)
- [x] No raw `<button>` or native checkbox inputs outside
      `src/components/ui` and `src/components/primitives` — justified
      exceptions opt out per line with a reason
- [x] No `.btn` class names, scoped to `className` attributes so a
      `data-testid` containing "btn" does not trip it
- [x] **No emoji or dingbat glyphs** in JSX text or string literals. They
      render as full-colour, double-width system glyphs beside lucide's thin
      monochrome strokes, and cannot inherit `currentColor`. Use
      `<Icon icon={...} />`. The ranges deliberately exclude U+2190–U+21FF
      (← ↑ → ↓ ↵), which are legitimate in diff summaries and keyboard hints

Not enforced, and deliberately so:

- **Colour-role allocation** — red only for destructive, one primary per
  screen, green as dot-not-fill — is a contract *this document* carries, and
  `eslint.config.js` says so where the rule would otherwise go.
  `no-restricted-syntax` carries one severity per file, and the rules above
  are errors; a warn-level colour regex cannot coexist with them, and this
  repo has a recorded history of className-regex false positives. **So this
  section is the only place the colour contract exists. It cannot be deleted
  without deleting the contract.**

Still open, and unclaimed by anything:

- [ ] No raw hex colours in TSX files (use CSS variables)
- [ ] Stylelint: no undefined CSS variables

---

## Design language (Restyle, 2026-08)

The visual language every screen implements. Two implementers following
these rules should produce the same UI.

### Borders

Borders exist for: form controls (on `bg-background-input`), focus rings,
the `outline` Button variant, and hairline `border-border-light` dividers
that anchor a sticky header/footer over a scroll region. **Everything else
separates by surface step + spacing.** In-flow cards, panels, chips, and
banners are borderless — a bordered element reads as interactive.

### Surface ladder

- L0 canvas: `bg-background` (app header `bg-background-elevated`)
- L1 in-flow panel/card: `bg-surface`
- L2 static chips, code/meta blocks: `bg-surface-elevated`
- Floating surfaces (popover/modal/dropdown): `bg-surface-elevated` +
  `shadow-lg`/`shadow-2xl` — the shadow tokens carry their own hairline ring
- Hover: `bg-surface-hover` on surfaces, `bg-background-hover` on canvas rows

### Selection — one idiom

Vertical list items use the model-row pattern: an always-present transparent
`border-l-[3px]`, selected = `border-l-primary` + `bg-primary-subtle`,
running = `border-l-success`. The accent never shifts layout. Grid cards
(HF browser) use `bg-primary-subtle` + `ring-1 ring-primary-border`.
Full `border-primary` selection treatments are retired.

### Tabs

One treatment via `ui/Tabs`: inactive `text-text-muted`, active `text-text`
(not accent-colored) with a 2px `bg-primary` underline bar. Every tablist
has an accessible name.

### Color allocation

- Exactly **one solid `primary` Button per visible surface**.
- Solid red (`Button variant="danger"`) only inside confirm dialogs;
  destructive actions elsewhere are `dangerGhost` (muted at rest, red on hover).
- Green means running/online only, expressed as a **dot + neutral text** —
  never a filled pill.
- Amber (`primary`) is selection, focus, links, and the single primary CTA.
- Warning is always an icon or label **plus text**, never color alone —
  accent amber and warning gold are hue neighbors by design, so the words
  carry the distinction.

### Type hierarchy

No all-caps micro-labels. Panel/modal titles `text-lg font-semibold`;
section headings `text-sm font-semibold text-text`; body and controls
`text-sm`; meta `text-xs text-text-muted`; ports/paths/quant strings and
**all numeric telemetry** (counts, rates, percentages, sizes)
`font-mono tabular-nums`. `text-2xs` is reserved for badge numerals and
readout unit suffixes.

### Readouts & live charts

Every live metric renders through `primitives/Readout`: mono `tabular-nums`
value, quiet unit suffix, label in `text-xs text-text-muted`. Intent colors
the value only — labels and units always stay quiet, and warning/danger
intents never appear without the label naming the metric.

Exactly **one sparkline style app-wide**: `primitives/Sparkline` with its
defaults (currentColor stroke, 2px terminus dot, faint `stroke-border`
baseline) — no per-screen variants; only width/height flex with context.
A sparkline never appears without an adjacent current-value Readout.
Percentage series pin the domain (`min={0} max={100}`) so lines show real
movement, not autoscaled noise.

Usage-meter severity thresholds (donut, KV readouts) are **70 warning /
90 danger**; below 70 meters wear `primary` — green is reserved for
running/online and never means "usage is fine". The donut takes its percent
and its severity from `utils/contextUsage.ts`, so its color and its figure
cannot disagree. A compact meter with no room for a figure (the chat
composer's context ring) shows a warning icon and the figure beside it from
70%, so it is never color alone.

### Density & radius

List rows `py-sm px-md`; form sections separate with `gap-xl` + a heading,
not `border-t` divider walls. Radii: controls `rounded-base`, in-flow cards
`rounded-md`, floating surfaces `rounded-lg`, dots/badges `rounded-full`.

### Primitives, not markup

Never render a raw `<button>` or `<input type="checkbox">` outside
`src/components/ui` / `src/components/primitives` — use `Button`,
`IconButton`, `Tabs`, `Chip`, `Banner`, `Checkbox`. Enforced by ESLint;
justified exceptions carry an inline disable with a reason.

<!-- module-docs:end -->
