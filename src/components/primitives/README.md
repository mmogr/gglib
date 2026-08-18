# primitives

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-components-primitives-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-components-primitives-complexity.json)

<!-- module-docs:start -->

Low-level layout and display primitives built on Tailwind design tokens. These are the foundational building blocks used throughout all other components — pure composition, no business logic, no state.

## Key Files

| File | Role |
|------|------|
| `Stack.tsx` | Vertical flex container; `gap`, `align`, `justify` props |
| `Row.tsx` | Horizontal flex container; same props as `Stack` |
| `Readout.tsx` | Label, value and quiet unit; `intent` colours the value only |
| `Sparkline.tsx` | Inline trend line over a series of numbers; configurable width and height |
| `Label.tsx` | Semantic `<label>` or `<span>`; sizes: `xs`, `sm`, `base`; `muted` for secondary text |
| `EmptyState.tsx` | Centred placeholder with icon slot, title, description, optional action |
| `Skeleton.tsx` | Shimmer-animated placeholder; variants: `text`, `rect`, `circle`; `count` for multiples |

All primitives accept `className` for ad-hoc overrides and use `cn()` for Tailwind class merging.

<!-- module-docs:end -->
