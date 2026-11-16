<!-- module-docs:start -->

Tauri command handlers.

After Phase 3 HTTP API consolidation, only OS-specific commands remain:
- util: API discovery (`get_embedded_api_info`, which answers once the app's launch of the daemon has settled), menu sync, OS integration, showing the main window (the tray panel's Open gglib), and starting the daemon (the main window's Start gglib, the tray menu's own `Daemon::restart`)
- `app_logs`: Frontend-to-backend logging bridge

All business logic is exposed via HTTP API (gglib-axum).
See scripts/check-tauri-commands.sh for enforcement.

<!-- module-docs:end -->
