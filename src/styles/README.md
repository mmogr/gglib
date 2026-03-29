# Styling & UI Architecture Contracts

**Version**: Phase 0 (foundational contracts)  
**Status**: Active  
**Epic**: [#19 - Tailwind-first UI rewrite](https://github.com/mmogr/gglib/issues/19)

---

## Overview

This document defines the **non-negotiable contracts** for gglib's Tailwind-first UI architecture. These rules govern how we build, style, and organize UI components across both **Tauri desktop** and **Axum WebUI** platforms.

**Critical principle**: This is a **clean-slate migration** with **no backwards compatibility**. When we introduce new primitives in Phases 1–5, we migrate all usages immediately and **delete legacy assets**.

---

## 1. Design Tokens: Source of Truth

### Contract

**CSS variables in [`variables.css`](base/variables.css) are the canonical design token source.**

- All design tokens (colors, spacing, typography, shadows, etc.) are defined as CSS variables in `:root`
- Tailwind consumes these tokens via `@theme inline` configuration in [`tailwind.css`](tailwind.css)
- No parallel token systems—CSS variables are the single source of truth
- Token changes propagate automatically to both Tailwind utilities and vanilla CSS

### Token Organization

Tokens follow a **semantic layering** approach:

```css
/* Foundation tokens (primitives) */
--color-primary: #3b82f6;
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

### Migration Path

- **Phase 0**: All tokens defined, gaps filled
- **Phase 4**: Token hygiene audit — ✅ **COMPLETE**. All semantic subtle-tint and border tokens added (`--color-{primary,success,warning,danger}-subtle` and `--color-{primary,success,warning,danger}-border`). Surface alias `--color-surface-raised` added. Bridged to Tailwind @theme.
- **Phase 5**: Enforce via linting

### Component Color Rule (enforced as of Phase 4)

> **No raw `rgba()` or `#hex` color values in component files.**
>
> All color references must use one of:
> - A Tailwind semantic utility class (e.g. `bg-danger-subtle`, `text-success`, `border-primary-border`)
> - A CSS variable reference (e.g. `var(--color-danger-subtle)`) — only when a Tailwind utility is unavailable

Inline arbitrary values like `bg-[rgba(239,68,68,0.15)]` or `text-[#ef4444]` are **banned**. Add tokens to `variables.css` instead.

---

## 2. Tailwind Scope & CSS Modules Policy

### Contract

**Tailwind is the default for layout and component composition.**

### When to Use Tailwind

✅ **Default choice for:**
- Layout primitives (flex, grid, spacing)
- Component composition (containers, wrappers, cards)
- Interactive states (hover, focus, active)
- Responsive design (breakpoints, conditional styles)
- Utility-first styling in TSX files

```tsx
// ✅ Tailwind-first approach
<button className="flex items-center gap-2 px-4 py-2 bg-primary hover:bg-primary-hover rounded-md">
  <Icon name="plus" size={16} />
  Add Item
</button>
```

### When to Use CSS Modules

✅ **Allowed for:**
- Truly component-unique styling that doesn't map to utilities
- Complex animations requiring `@keyframes`
- Legacy components **during migration only** (temporary)

❌ **Not allowed for:**
- Layout that can be expressed with Tailwind utilities
- Simple hover/focus states
- Colors, spacing, typography already in design tokens
- New components after Phase 0 (unless justified)

### File Size Budget

**Complexity budget guideline:**
- TSX files: **≤200 LOC** per component (exceptions must be justified)
- CSS files: **≤200 LOC** per module (split by subcomponent/concern if larger)
- If a component exceeds this, decompose into smaller single-purpose components

---

## 3. No-Compatibility Deletion Policy

### Contract

**When a new primitive is introduced, we migrate all usages immediately and delete legacy equivalents. No gradual deprecation.**

### Phase-by-Phase Deletion

| Phase | New Primitives | Legacy Assets Deleted |
|-------|----------------|----------------------|
| **Phase 1** | `Button`, `Icon` (final versions) | `buttons.css`, all `.btn` class usages |
| **Phase 2** | `Input`, `Select`, `Textarea`, form components | `forms.css`, all `.form-input` usages, CSS Module form clones |
| **Phase 3** | Layout primitives (Stack, Grid, Container) | Inline layout styles, god components split |
| **Phase 4** | Token-aligned color system | Raw hex colors in components — ✅ **COMPLETE** |
| **Phase 5** | Final cleanup | Any remaining CSS Modules not justified, dead CSS |

### Why No Compatibility?

- **Prevents drift**: Can't have "old way" and "new way" coexisting
- **Forces migration**: Immediate migration ensures no forgotten usages
- **Reduces complexity**: One way to do things, documented and enforced
- **Faster completion**: Aggressive deletion accelerates the rewrite

### Progressive Adoption (Phase 0 Only)

**During Phase 0 only**, existing components keep their current styling approach. However:
- **New components** created after Phase 0 **must** use Tailwind-first approach
- **Modified components** should opportunistically migrate to Tailwind utilities where trivial
- **No new CSS Modules** should be created without justification

**Starting Phase 1**: All migration is mandatory and immediate.

---

## 4. Platform Parity Requirements

### Contract

**UI must render identically in Tauri desktop and Axum WebUI. Shared UI components must be platform-agnostic.**

### Platform Architecture

```
┌────────────────────────────────────────────────┐
│           Shared UI Components                 │
│        (src/components, src/pages)             │
│                                                │
│  • Must be platform-agnostic                   │
│  • No direct Tauri API imports                 │
│  • Inject platform deps via props/context      │
└────────────────┬───────────────────────────────┘
                 │
                 │ Import from adapter
                 ▼
┌────────────────────────────────────────────────┐
│       Platform Adapter Interface               │
│        (src/services/platform)                 │
│                                                │
│  • Platform detection (isTauri, isWeb)         │
│  • File dialogs                                │
│  • Native menus                                │
│  • External URL opening                        │
│  • Event streaming (Tauri events vs SSE)       │
│                                                │
│  TRANSPORT_EXCEPTION: unavoidable platform code│
└────────────────┬───────────────────────────────┘
                 │
         ┌───────┴────────┐
         ▼                ▼
    ┌─────────┐      ┌──────────┐
    │  Tauri  │      │ Axum Web │
    │ Desktop │      │   UI     │
    └─────────┘      └──────────┘
```

### Rules for Shared UI

✅ **Allowed in shared UI:**
- React components, hooks, contexts
- Styling (Tailwind, CSS Modules, CSS variables)
- Import from `services/platform/*` adapters
- Props for injecting platform-specific functionality

❌ **Not allowed in shared UI:**
- Direct imports from `@tauri-apps/api`
- Direct imports from `@tauri-apps/plugin-*`
- `window.__TAURI__` checks (use `services/platform/detect.ts` instead)
- Platform-specific business logic

### Platform-Specific Code Location

All platform-specific implementations must live in:

**`src/services/platform/`**

Example files:
- `detect.ts` - Platform detection utilities
- `fileDialogs.ts` - Native file picker (Tauri) vs HTML input (Web)
- `llamaBinary.ts` - llama.cpp binary management (Tauri only)
- `menu.ts` - Native menu integration (Tauri only)
- `openUrl.ts` - External URL opening
- `serverLogs.ts` - Log streaming (Tauri events vs SSE)

### TRANSPORT_EXCEPTION Marker

Use `// TRANSPORT_EXCEPTION` comments to mark code where platform-specific behavior is unavoidable:

```typescript
// TRANSPORT_EXCEPTION: Tauri uses native events, Web uses SSE
if (isTauri()) {
  await listen('llama-stdout', handleLog);
} else {
  const eventSource = new EventSource('/api/logs');
  eventSource.onmessage = handleLog;
}
```

### Testing Platform Parity

**Manual verification checklist:**

1. **Run Tauri desktop**: `npm run tauri:dev`
2. **Run Axum WebUI**: `cargo run --package gglib-cli -- web --api-only --port 9887` + `npm run dev`
3. **Test UI features**:
   - Button styles and interactions (hover, active, disabled)
   - Modal dialogs (open, close, backdrop click)
   - Form inputs (focus, validation, error states)
   - Layout responsiveness (resize window)
   - Icon rendering
4. **Visual comparison**: Take screenshots, ensure pixel-perfect match where platform allows

---

## 5. Tailwind v4 Configuration

### Current Setup

Tailwind v4 uses **CSS-native configuration** (no `tailwind.config.js`).

**File**: [`tailwind.css`](tailwind.css)

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

### Why `@theme inline`?

- **Prevents circular references**: `@theme inline` properly references external CSS variables defined in `:root`
- **Enables both utility classes and vanilla CSS**: Tailwind generates utilities like `bg-primary` while vanilla CSS can still use `var(--color-primary)`
- **Avoids resolution issues**: Ensures CSS variable values are correctly resolved when nested

### Usage Patterns

```tsx
// ✅ Tailwind utility classes
<div className="bg-primary text-text border-border" />

// ✅ Arbitrary values with CSS variables
<div className="bg-[var(--color-primary-hover)]" />

// ✅ Vanilla CSS in .module.css
.myClass {
  background: var(--color-primary);
}

// All three work together seamlessly!
```

---

## 6. File Organization Conventions

### Component Structure

```
src/
├── components/
│   ├── ui/                    # UI primitives (Button, Icon, Modal, Input)
│   │   ├── Button.tsx
│   │   ├── Icon.tsx
│   │   └── Modal.tsx
│   ├── AddModel.tsx           # Feature components
│   ├── AddModel.module.css    # Collocated CSS Module (if needed)
│   └── Header.tsx
├── pages/                     # Route/page components
├── contexts/                  # React contexts
├── hooks/                     # Custom hooks
├── services/
│   ├── platform/              # Platform-specific adapters
│   └── api/                   # Backend API clients
├── styles/
│   ├── base/
│   │   ├── variables.css      # Design tokens (source of truth)
│   │   ├── reset.css
│   │   └── typography.css
│   ├── components/            # Global component CSS (legacy, will be deleted)
│   │   ├── buttons.css        # ❌ Delete in Phase 1
│   │   ├── forms.css          # ❌ Delete in Phase 2
│   │   └── modals.css         # ✅ Used by Modal.tsx (Phase 2 migration)
│   ├── tailwind.css           # Tailwind v4 configuration
│   └── main.css               # Global application styles
└── types/                     # TypeScript types
```

### Import Conventions

```typescript
// React
import { useState, useEffect } from 'react';

// UI primitives
import { Button } from './ui/Button';
import { Icon } from './ui/Icon';

// Icons
import { Plus, Check, X } from 'lucide-react';

// Platform adapters
import { isTauri } from '../services/platform/detect';
import { openFileDialog } from '../services/platform/fileDialogs';

// Styles
import styles from './Component.module.css';
```

---

## 7. Phase Roadmap

| Phase | Focus | Status | Issue |
|-------|-------|--------|-------|
| **Phase 0** | Contracts, token fixes, platform parity docs | ✅ Complete | [#14](https://github.com/mmogr/gglib/issues/14) |
| **Phase 1** | Button + Icon primitives migration, delete `buttons.css` | 🔄 Blocked | [#16](https://github.com/mmogr/gglib/issues/16) |
| **Phase 2** | Input/Form primitives migration, delete `forms.css` | 🔄 Blocked | [#13](https://github.com/mmogr/gglib/issues/13) |
| **Phase 3** | Layout primitives, decompose god components | 🔄 Blocked | [#18](https://github.com/mmogr/gglib/issues/18) |
| **Phase 4** | Token hygiene, no raw hex colors | 🔄 Blocked | [#15](https://github.com/mmogr/gglib/issues/15) |
| **Phase 5** | Final cleanup, add guardrails, parity smoke tests | 🔄 Blocked | [#17](https://github.com/mmogr/gglib/issues/17) |

**Dependency order:**
- Phase 0 unblocks all phases
- Phase 1 blocks Phase 2
- Phases 1-2 unblock Phase 3
- Phase 4 runs alongside Phases 1-3
- Phase 5 is final (cleanup and enforcement)

---

## 8. Enforcement & Validation

### During Development

- **Code review checklist**: PR reviewers verify compliance with contracts
- **Self-check before commit**: Does this follow Tailwind-first? Are CSS variables used? Is platform code isolated?

### Automated Enforcement (Phase 5)

- [ ] ESLint rule: No direct `@tauri-apps/api` imports in `src/components`
- [ ] ESLint rule: No raw hex colors in TSX files (use CSS variables)
- [ ] Stylelint rule: No undefined CSS variables
- [ ] Pre-commit hook: Run `complexity_hotspots.sh` to flag >200 LOC files
- [ ] CI check: Verify `buttons.css` and `forms.css` deleted after Phase 2

---

## 9. Migration Checklist (For Phases 1-5)

When migrating a component:

- [ ] Replace CSS Module/global CSS with Tailwind utilities where possible
- [ ] Use CSS variables for colors, spacing, typography (no raw hex)
- [ ] Ensure component is platform-agnostic (no Tauri API imports)
- [ ] Keep file ≤200 LOC (split if needed)
- [ ] Test in both Tauri and Axum WebUI
- [ ] Delete legacy CSS file once migration complete
- [ ] Update all call sites to use new primitive

---

## Questions & Feedback

For questions about these contracts or proposed changes:
- Open an issue tagged with `component: frontend` and `arch: domain`
- Reference this document and the specific section
- Propose alternatives with justification

**These contracts are living documentation**—they may evolve based on learnings from Phases 1-5, but changes require explicit discussion and approval.
