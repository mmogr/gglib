<!-- module-docs:start -->

Tauri command handlers.

After Phase 3 HTTP API consolidation, only OS-specific commands remain:
- llama: Binary installation and status checks
- util: API discovery, menu sync, OS integration
- app_logs: Frontend-to-backend logging bridge

All business logic is exposed via HTTP API (gglib-axum).
See scripts/check-tauri-commands.sh for enforcement.

<!-- module-docs:end -->

<details>
<summary><h2>Modules</h2></summary>

<!-- module-table:start -->
| Module | LOC | Complexity | Coverage |
|--------|-----|------------|----------|
<!-- module-table:end -->

</details>
