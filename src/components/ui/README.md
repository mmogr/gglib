# ui

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-components-ui-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-components-ui-complexity.json)

<!-- module-docs:start -->

High-level interactive UI primitives: `Button`, `Input`, `Modal`, `Select`, `Textarea`, `Icon`, and `ConfirmDialog`. Built on Radix UI primitives and styled with Tailwind design tokens. Every clickable or form-input element in the app uses components from here.

## Key Files

| File | Role |
|------|------|
| `Button.tsx` | Variants: `primary`, `secondary`, `ghost`, `outline`, `danger`, `success`, `warning`, `link`; sizes: `sm`, `md`, `lg`; `isLoading` spinner |
| `Input.tsx` | Text input with optional icon slots; variants: `default`, `error` |
| `Modal.tsx` | Radix `Dialog` wrapper; sizes: `sm`, `md`, `lg`; `preventClose` option; typed `footer` slot |
| `Select.tsx` | Native `<select>` with consistent styling |
| `Textarea.tsx` | Multi-line input with same variant/size system as `Input` |
| `Icon.tsx` | Lucide icon wrapper with standardised `size` prop |
| `ConfirmDialog.tsx` | Confirm/cancel dialog for destructive action confirmation |

All components accept `className` for overrides and use `cn()` for Tailwind class merging.

<!-- module-docs:end -->
