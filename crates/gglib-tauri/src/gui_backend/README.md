# gui_backend

<!-- module-docs:start -->

GUI backend bridge module.

This module is a thin bridge that re-exports types and the `GuiBackend` from the shared `gglib-gui` crate. Tauri command handlers import from here, maintaining existing import paths while delegating to shared code.

## Re-exports

| Type | Source |
|------|--------|
| `GuiBackend` | `gglib-gui` |
| `GuiDeps` | `gglib-gui` |
| `GuiError` | `gglib-gui` |
| `QueueSnapshot` | `gglib-core` |
| `ModelFilterOptions` | `gglib-core` |

## Design

This pattern allows `gglib-gui` to contain the shared backend logic while `gglib-tauri` focuses on Tauri-specific command registration.

<!-- module-docs:end -->

<details>
<summary><h2>Modules</h2></summary>

<!-- module-table:start -->
| Module | LOC | Complexity | Coverage |
|--------|-----|------------|----------|
<!-- module-table:end -->

</details>
