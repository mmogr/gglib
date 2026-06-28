# FilterPopover

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-components-FilterPopover-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-components-FilterPopover-complexity.json)

<!-- module-docs:start -->

Popover containing model library filter controls: sort field and direction selectors, dual-handle `RangeSlider` components for parameter count, context length, and token generation speed, plus checkboxes for quantization types and user tags.

## Key Files

| File | Role |
|------|------|
| `FilterPopover.tsx` | Full filter UI; delegates slider rendering to `RangeSlider`; close-on-outside-click |

Value formatters (`formatParamCount`, `formatContextLength`, `formatSpeed`) convert raw numbers to human-readable labels (e.g., `7B`, `128k`, `45 t/s`) for slider thumb display.

<!-- module-docs:end -->
