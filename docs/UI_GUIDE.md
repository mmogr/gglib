# UI Contribution Guide

## Philosophy

GGLib follows a **clean-slate, Tailwind-first** UI architecture. We prioritize **DRY (Don't Repeat Yourself)**, **modularity**, and **small file sizes** to maintain a low-complexity codebase.

## Core Principles

### 1. Compose from Primitives First

**Always use existing primitives** before creating new components:

- **Layout:** `Stack`, `Row`, `Card`, `EmptyState` from `src/components/primitives/`
- **Forms:** `Input`, `Textarea`, `Select`, `Button` from `src/components/ui/`
- **Modals:** `Modal` from `src/components/ui/Modal.tsx`

**Example - Bad:**
```tsx
<div style={{ display: 'flex', flexDirection: 'column', gap: '16px' }}>
  <div style={{ display: 'flex', gap: '8px' }}>
    <button className="my-button">Submit</button>
  </div>
</div>
```

**Example - Good:**
```tsx
<Stack gap="base">
  <Row gap="sm">
    <Button variant="primary">Submit</Button>
  </Row>
</Stack>
```

### 2. Keep Files Small

**Target:** 200-300 LOC per file (enforced by `check_file_complexity.sh`)

**When a file exceeds 300 LOC:**
1. Extract focused subcomponents into a subdirectory
2. Keep the main file as a coordinator
3. Use barrel exports (`index.ts`) for clean imports

**Example structure:**
```
SettingsModal/
  ├── SettingsModal.tsx (267 LOC - coordinator)
  ├── GeneralSettings.tsx (308 LOC - form logic)
  └── index.ts (exports both)
```

### 3. Use Tokens, Not Raw Values

**Always use CSS variables or Tailwind classes** for spacing, colors, and sizing:

**Bad:**
```tsx
<div style={{ marginTop: '16px', color: '#4a9eff' }}>
```

**Good:**
```tsx
<div className="mt-4 text-[var(--color-primary)]">
```

**Available tokens:**
- **Spacing:** `--spacing-xs` through `--spacing-3xl` (use Tailwind: `p-2`, `mt-4`, `gap-6`)
- **Colors:** `--color-text`, `--color-background`, `--color-primary`, etc.
- **Radii:** `--radius-sm`, `--radius-base`, `--radius-lg`

### 4. Avoid Inline Styles

Inline styles make components harder to theme and maintain.

**Only use inline styles for:**
- **Truly dynamic values** (e.g., `width: ${progress}%`)
- **Computed positions** (e.g., `top: ${mouseY}px`)

**If you must use inline styles**, add a TODO comment:
```tsx
// TODO: Extract to CSS module once positioning logic is finalized
<div style={{ transform: `translate(${x}px, ${y}px)` }}>
```

### 5. Platform Parity

GGLib runs in **both Tauri (desktop) and Axum WebUI (browser)**. Ensure your UI works identically in both:

- Use **platform-agnostic components** (no `window.electron` or Tauri-specific APIs in UI)
- Platform-specific behavior goes in **adapter layers** (e.g., `src/services/transport/`)
- Test in both modes: `npm run tauri:dev` and `npm run dev`

## File Structure Conventions

### Component Organization

```
src/components/
  ├── primitives/          # Reusable layout primitives
  │   ├── Card.tsx
  │   ├── Stack.tsx
  │   ├── Row.tsx
  │   └── EmptyState.tsx
  ├── ui/                  # Form controls & modals
  │   ├── Button.tsx
  │   ├── Input.tsx
  │   └── Modal.tsx
  ├── [FeatureName]/       # Feature-specific components
  │   ├── index.ts         # Barrel export
  │   ├── [FeatureName].tsx
  │   ├── [FeatureName].module.css
  │   └── components/      # Subcomponents
  │       ├── Subcomponent1.tsx
  │       └── Subcomponent2.tsx
```

### CSS Module Naming

- **Use CSS Modules** for component-specific styles: `Component.module.css`
- **Never use global classes** like `.btn`, `.form-input`, etc. (use primitives)
- **Keep CSS modules small** (<300 LOC) - extract subcomponents if needed

## Workflow

### Adding a New UI Feature

1. **Check for existing primitives** - Can you compose from `Stack`, `Row`, `Card`, `Button`?
2. **Sketch the component hierarchy** - Plan subcomponents if LOC exceeds 200
3. **Implement incrementally:**
   - Create the main component with primitives
   - Extract subcomponents as you go
   - Add CSS modules only for truly custom styling
4. **Verify platform parity:**
   ```bash
   npm run dev              # Test in browser (Axum)
   npm run tauri:dev        # Test in Tauri (desktop)
   ```
5. **Run guardrails:**
   ```bash
   npm run lint             # ESLint architecture rules
   ./scripts/check_file_complexity.sh  # File size check
   ```

### Refactoring an Existing Component

1. **Identify code smells:**
   - File >300 LOC
   - Duplicated layout patterns (candidate for primitives)
   - Inline styles for static values
   - Raw hex colors instead of tokens
2. **Extract subcomponents:**
   - Create `ComponentName/` directory
   - Move focused logic to `ComponentName/Subcomponent.tsx`
   - Update main file to use extracted components
3. **Replace inline styles:**
   - Replace `style={{ display: 'flex' }}` with `<Stack>` or Tailwind classes
   - Replace `style={{ marginTop: '16px' }}` with `className="mt-4"`
4. **Verify build:**
   ```bash
   npm run build
   ```

## ESLint Rules (Enforced)

- **Forbid legacy CSS classes:** `.btn`, `.m-xs`, `.mt-base`, etc. (use primitives/Tailwind)
- **Warn on inline styles:** Use Tailwind or CSS modules (or add TODO comment)
- **React Hooks rules:** `rules-of-hooks` and `exhaustive-deps`

## CI Guardrails

The following checks run in CI to prevent regressions:

1. **`npm run lint`** - ESLint architecture rules
2. **`./scripts/check_file_complexity.sh`** - File size budget (300 LOC)
3. **`npm run build`** - TypeScript compilation + Vite build
4. **Platform parity smoke tests** (manual verification recommended)

## Examples

### Good: Composing from Primitives

```tsx
import { Stack, Row, Card } from "@/components/primitives";
import { Button, Input } from "@/components/ui";

export function MyForm() {
  return (
    <Card variant="elevated" padding="lg">
      <Stack gap="base">
        <h2 className="text-xl font-semibold">Settings</h2>
        
        <Stack gap="sm">
          <label>Name</label>
          <Input placeholder="Enter name" />
        </Stack>

        <Row gap="sm" justify="end">
          <Button variant="secondary">Cancel</Button>
          <Button variant="primary">Save</Button>
        </Row>
      </Stack>
    </Card>
  );
}
```

### Bad: Raw Divs and Inline Styles

```tsx
export function MyForm() {
  return (
    <div style={{ 
      background: '#2a2a2a', 
      padding: '24px', 
      borderRadius: '8px' 
    }}>
      <div style={{ display: 'flex', flexDirection: 'column', gap: '16px' }}>
        <h2 style={{ fontSize: '20px', fontWeight: 600 }}>Settings</h2>
        
        <div style={{ display: 'flex', flexDirection: 'column', gap: '8px' }}>
          <label>Name</label>
          <input style={{ padding: '8px' }} />
        </div>

        <div style={{ display: 'flex', gap: '8px', justifyContent: 'flex-end' }}>
          <button className="btn btn-secondary">Cancel</button>
          <button className="btn btn-primary">Save</button>
        </div>
      </div>
    </div>
  );
}
```

## Questions?

- Check existing components in `src/components/` for patterns
- Review recent PRs for refactoring examples
- Ask in GitHub discussions if unclear

**Remember:** The goal is **consistency, simplicity, and maintainability**. When in doubt, compose from primitives and keep files small! 🎨
