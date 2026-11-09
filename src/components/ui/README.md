# ui

<!-- module-docs:start -->

High-level interactive UI primitives: `Button`, `IconButton`, `Menu`, `Tabs`, `Chip`, `Banner`, `Checkbox`, `Input`, `SecretInput`, `SecretField`, `Modal`, `Select`, `Textarea`, `Icon`, `ConfirmDialog`, and `Listbox`. Built on Radix UI primitives and styled with Tailwind design tokens. Every clickable or form-input element in the app uses components from here — enforced by ESLint (`no-restricted-syntax` bans raw `<button>`/checkbox inputs outside this directory).

## Key Files

| File | Role |
|------|------|
| `Button.tsx` | Variants: `primary`, `secondary`, `ghost`, `outline`, `danger` (solid — confirm dialogs only), `dangerGhost`, `link`; sizes: `sm`, `md`, `lg`; `isLoading` spinner |
| `IconButton.tsx` | Icon-only Button; requires a `label` (aria-label + tooltip) |
| `Menu.tsx` | A button that opens a short menu beside it (under it, or over it with `side="above"` for the composer), moved here from `places/library` for the library header, the inspector's and the log's ⋯, a chat's title ▾ and ⋯, and the composer's `[+]` and model ▾: actions, ticked choices, boxes ticked on and off that leave it open, submenus (▸) beside their item, headings that name a group of items, and notes that are only text; danger items drawn as such; a disabled item's reason said under it and described by it, not only on hover; every button `type="button"`, so none submits a form it sits in; closes on a pick, a click outside or Escape, a pick and Escape giving the focus back to its button, and Escape in an open submenu closing that submenu alone, the focus on its item |
| `Tabs.tsx` | The one tab treatment: muted inactive, neutral active text, primary underline bar; requires a tablist `aria-label` |
| `Chip.tsx` | Inline metadata/filter token; renders a `<span>` when non-interactive, `<button>` with `onClick`; optional `onRemove` control |
| `Banner.tsx` | Borderless tinted status callout (`info`/`success`/`warning`/`danger`) with icon, optional `action`/`onDismiss` |
| `Checkbox.tsx` | sr-only input + styled peer box; `label`/`description` slots |
| `Input.tsx` | Text input with optional icon slots; variants: `default`, `error` |
| `SecretInput.tsx` | The one input a secret is typed into: masked, with an eye button beside it that shows what was typed |
| `SecretField.tsx` | A stored secret in a form (ADR 0027): `MASK` while held, Show and Copy through the daemon's one reveal, Replace, and Clear where allowed; the `SecretEdit` it hands back is keep, replace or clear, and `editedValue` is what a request sends for it |
| `Modal.tsx` | Radix `Dialog` wrapper; sizes: `sm`, `md`, `lg`; `height="fixed"`; `preventClose`; typed `subHeader`/`footer` slots |
| `Select.tsx` | Native `<select>` with token-styled chevron overlay |
| `Textarea.tsx` | Multi-line input with same variant/size system as `Input` |
| `Icon.tsx` | Lucide icon wrapper with standardised `size` prop |
| `ConfirmDialog.tsx` | Confirm/cancel dialog for destructive action confirmation |
| `Listbox.tsx` | Rows moved through with the arrow keys: one Tab stop for the list (the picked row's, else the first, `tabStop`), ↑ ↓ Home End moving the focus and the pick; `listMove`, where those keys go, is the Load a model… palette's too |

All components accept `className` for overrides and use `cn()` for Tailwind class merging.

<!-- module-docs:end -->
