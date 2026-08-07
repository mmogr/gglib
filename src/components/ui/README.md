# ui

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-components-ui-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-components-ui-complexity.json)

<!-- module-docs:start -->

High-level interactive UI primitives: `Button`, `IconButton`, `Tabs`, `Chip`, `Banner`, `Checkbox`, `Input`, `Modal`, `Select`, `Textarea`, `Icon`, and `ConfirmDialog`. Built on Radix UI primitives and styled with Tailwind design tokens. Every clickable or form-input element in the app uses components from here — enforced by ESLint (`no-restricted-syntax` bans raw `<button>`/checkbox inputs outside this directory).

## Key Files

| File | Role |
|------|------|
| `Button.tsx` | Variants: `primary`, `secondary`, `ghost`, `outline`, `danger` (solid — confirm dialogs only), `dangerGhost`, `link`; sizes: `sm`, `md`, `lg`; `isLoading` spinner |
| `IconButton.tsx` | Icon-only Button; requires a `label` (aria-label + tooltip) |
| `Tabs.tsx` | The one tab treatment: muted inactive, neutral active text, primary underline bar; requires a tablist `aria-label` |
| `Chip.tsx` | Inline metadata/filter token; renders a `<span>` when non-interactive, `<button>` with `onClick`; optional `onRemove` control |
| `Banner.tsx` | Borderless tinted status callout (`info`/`success`/`warning`/`danger`) with icon, optional `action`/`onDismiss` |
| `Checkbox.tsx` | sr-only input + styled peer box; `label`/`description` slots |
| `Input.tsx` | Text input with optional icon slots; variants: `default`, `error` |
| `Modal.tsx` | Radix `Dialog` wrapper; sizes: `sm`, `md`, `lg`; `height="fixed"`; `preventClose`; typed `subHeader`/`footer` slots |
| `Select.tsx` | Native `<select>` with token-styled chevron overlay |
| `Textarea.tsx` | Multi-line input with same variant/size system as `Input` |
| `Icon.tsx` | Lucide icon wrapper with standardised `size` prop |
| `ConfirmDialog.tsx` | Confirm/cancel dialog for destructive action confirmation |

All components accept `className` for overrides and use `cn()` for Tailwind class merging.

<!-- module-docs:end -->
