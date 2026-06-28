# RangeSlider

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-components-RangeSlider-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-components-RangeSlider-complexity.json)

<!-- module-docs:start -->

Dependency-free dual-handle range slider using two overlapping `<input type="range">` elements. The visual track fill is computed from handle positions and applied via inline `background-size`.

## Key Files

| File | Role |
|------|------|
| `RangeSlider.tsx` | Two overlapping range inputs; percentage-based track fill; `formatValue` prop for custom labels |
| `RangeSlider.css` | WebKit track appearance reset; thumb sizing and pointer cursor |

## Props

| Prop | Role |
|------|------|
| `min` / `max` | Absolute bounds |
| `minValue` / `maxValue` | Current handle positions (controlled) |
| `step` | Increment granularity |
| `onChange(min, max)` | Called when either handle moves |
| `formatValue` | Optional formatter for display labels |

<!-- module-docs:end -->
