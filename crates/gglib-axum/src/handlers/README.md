# handlers

<!-- module-docs:start -->

HTTP request handlers for the Axum web server.

Handlers are organized into domain-scoped subdirectories:
- [`model`]  — CRUD, verification, downloads, `HuggingFace` discovery
- [`config`] — settings, system setup

`attachments.rs` is `POST /api/attachments` and `GET /api/attachments/{id}`:
an image the chat page sends once, as the raw body, and names by its id in
every message after. Both are thin over core's `AttachmentService`.

`chat_title.rs` is `POST /api/chat`: a chat's title, asked of the
llama-server the chat runs on. The body names the port, the messages, a
temperature and a token cap, and any other key is refused. The request runs
core's `request_pipeline` with thinking off, is never streamed, and is
answered with the model's text.

<!-- module-docs:end -->
