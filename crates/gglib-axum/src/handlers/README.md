# handlers

<!-- module-docs:start -->

HTTP request handlers for the Axum web server.

Handlers are organized into domain-scoped subdirectories:
- [`model`]  — CRUD, verification, downloads, `HuggingFace` discovery
- [`config`] — settings, system setup

`attachments.rs` is `POST /api/attachments` and `GET /api/attachments/{id}`:
an image the chat page sends once, as the raw body, and names by its id in
every message after. Both are thin over core's `AttachmentService`.

<!-- module-docs:end -->
