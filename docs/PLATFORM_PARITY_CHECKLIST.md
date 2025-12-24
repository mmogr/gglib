# Platform Parity Verification Checklist

**Last Updated:** December 24, 2025  
**Phase:** Phase 5 - Cleanup & Enforcement

## Test Environment

- **Web (Axum):** `npm run dev` → http://localhost:5173/
- **Desktop (Tauri):** `npm run tauri:dev`

## Core Primitives (Phase 3)

### Stack Component
- [ ] **Web:** Stack renders with correct vertical spacing (gap: xs/sm/base/lg/xl)
- [ ] **Tauri:** Same spacing behavior
- [ ] **Both:** Alignment options (start/center/end) work correctly
- [ ] **Both:** Nested Stacks don't create extra spacing

### Row Component
- [ ] **Web:** Row renders with correct horizontal spacing
- [ ] **Tauri:** Same spacing behavior
- [ ] **Both:** Wrapping enabled when `wrap` prop is true
- [ ] **Both:** Alignment and justification props work correctly

### Card Component
- [ ] **Web:** All variants render (default/elevated/outlined)
- [ ] **Tauri:** All variants render identically
- [ ] **Both:** Padding options (sm/base/lg/xl) match token values
- [ ] **Both:** Hover states work (if interactive)

### EmptyState Component
- [ ] **Web:** Icon, title, description render correctly
- [ ] **Tauri:** Same visual layout
- [ ] **Both:** Action button (if provided) is clickable

## Form Primitives (Phase 4)

### Input Component
- [ ] **Web:** Text inputs accept focus and typing
- [ ] **Tauri:** Text inputs accept focus and typing
- [ ] **Both:** Error states display correctly
- [ ] **Both:** Disabled state styling matches
- [ ] **Both:** Password inputs toggle visibility

### Textarea Component
- [ ] **Web:** Multi-line text entry works
- [ ] **Tauri:** Multi-line text entry works
- [ ] **Both:** Auto-resizing (if implemented) works

### Select Component
- [ ] **Web:** Dropdown opens and options are selectable
- [ ] **Tauri:** Dropdown opens and options are selectable
- [ ] **Both:** Selected value displays correctly

### Button Component
- [ ] **Web:** All variants render (primary/secondary/ghost)
- [ ] **Tauri:** All variants render identically
- [ ] **Both:** Loading spinner displays when `isLoading` is true
- [ ] **Both:** Disabled state prevents clicks

## Decomposed Components (Phase 3)

### SettingsModal
- [ ] **Web:** Modal opens with correct backdrop
- [ ] **Tauri:** Modal opens with correct backdrop
- [ ] **Both:** Tab navigation works (General/MCP Servers)
- [ ] **Both:** GeneralSettings subcomponent renders form fields
- [ ] **Both:** Form submission saves settings
- [ ] **Both:** Close button dismisses modal

### AddMcpServerModal
- [ ] **Web:** Modal opens for adding new server
- [ ] **Tauri:** Modal opens for adding new server
- [ ] **Both:** ServerTemplatePicker displays preset options
- [ ] **Both:** ServerTypeConfig switches between stdio/sse
- [ ] **Both:** EnvVarManager adds/removes environment variables
- [ ] **Both:** Form validation works (required fields)
- [ ] **Both:** Submission creates new MCP server

### HuggingFaceBrowser
- [ ] **Web:** Search input accepts text
- [ ] **Tauri:** Search input accepts text
- [ ] **Both:** Filter controls (task, sort, params) function
- [ ] **Both:** EmptyState displays when no results
- [ ] **Both:** Model cards display in Stack layout
- [ ] **Both:** Load More button fetches additional results
- [ ] **Both:** Error states display with EmptyState

## Modal System

### Modal Component
- [ ] **Web:** Modals center correctly
- [ ] **Tauri:** Modals center correctly
- [ ] **Both:** Backdrop dismisses modal on click (if enabled)
- [ ] **Both:** Esc key dismisses modal (if enabled)
- [ ] **Both:** Focus trap works (tab stays within modal)
- [ ] **Both:** Scrolling works for tall content

## Layout & Spacing

### Tailwind Classes
- [ ] **Web:** Margin classes (mt-2, mb-4, etc.) apply correctly
- [ ] **Tauri:** Margin classes apply identically
- [ ] **Both:** Padding classes work
- [ ] **Both:** Gap utilities in flex containers work
- [ ] **Both:** Responsive classes (sm:, md:, lg:) trigger at breakpoints (web only)

### CSS Tokens
- [ ] **Web:** --color-* tokens render correct colors
- [ ] **Tauri:** --color-* tokens render correct colors
- [ ] **Both:** --spacing-* tokens match expected sizes
- [ ] **Both:** --radius-* tokens round corners correctly
- [ ] **Both:** Dark mode tokens switch (if implemented)

## Known Platform Differences (Expected)

### Web-only Features
- **Responsive breakpoints:** Tailwind `sm:`/`md:`/`lg:` classes only apply in browser (Tauri has fixed window size)

### Tauri-only Features
- **Native OS chrome:** Tauri uses native window decorations (title bar, close button)
- **File system access:** Tauri can access local file system directly (web uses dialog APIs)

### Not Expected to Match
- **Scrollbar styling:** Tauri uses native scrollbars, web uses custom CSS
- **Font rendering:** Slight antialiasing differences between platforms
- **Window chrome:** Tauri has native OS window, web has browser chrome

## Testing Workflow

1. **Start both platforms:**
   ```bash
   # Terminal 1 - Web
   npm run dev
   
   # Terminal 2 - Tauri
   npm run tauri:dev
   ```

2. **Test core flows:**
   - Open settings modal → verify GeneralSettings renders
   - Add MCP server → verify all subcomponents render
   - Search HuggingFace → verify EmptyState and results
   - Create/edit models → verify form primitives work

3. **Document issues:**
   - Note any visual differences
   - Check browser console for errors (web)
   - Check Tauri DevTools console for errors (desktop)

4. **Sign off:**
   - [ ] All critical flows verified in both platforms
   - [ ] No blocking visual regressions
   - [ ] Forms submit correctly
   - [ ] Modals open/close correctly

## Sign-Off

**Tested by:** _________________  
**Date:** _________________  
**Platforms tested:** Web ☐ Tauri ☐  
**Result:** Pass ☐ / Fail ☐  
**Notes:** ___________________________________________________________________

