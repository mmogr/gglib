# base

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-styles-base-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-styles-base-complexity.json)

<!-- module-docs:start -->

Root CSS custom-property design token system establishing the application's complete visual language: colour palette, spacing scale, typography, border radii, shadows, transitions, and z-index tiers. Tailwind's `@theme inline` maps these tokens to utility classes, making every token available as both a CSS variable and a Tailwind utility.

## Key Files

| File | Role |
|------|------|
| `variables.css` | All `:root` CSS custom properties; organized into semantic sections |

## Token Categories

| Category | Examples |
|----------|---------|
| Colour — primitive | `--color-primary: #f08c26`, `--color-danger: #e05252` |
| Colour — semantic | `--color-background`, `--color-surface`, `--color-text-muted` |
| Colour — state tints | `--color-primary-subtle`, `--color-danger-border` |
| Spacing | `--spacing-xs: 4px` … `--spacing-2xl: 48px` |
| Typography | `--font-size-sm: 13px` … `--font-weight-semibold: 600` |
| Border radius | `--radius-sm: 4px` … `--radius-full: 9999px` |
| Shadows | `--shadow-sm` … `--shadow-2xl` (dark-mode tuned, 50% black opacity) |
| Z-index | `--z-base: 0`, `--z-popover`, `--z-modal` |

## Usage Rule

All colour values in component files must reference a token via `var(--color-*)` or a Tailwind semantic utility class. Raw `#hex` or `rgba()` values are prohibited — add a token to `variables.css` instead.

The one standing exception is `ConsoleLogPanel.css`, whose ANSI palette is a
fixed 16-colour standard rather than a design choice, and would mean sixteen
tokens nothing else could use.

<!-- module-docs:end -->
