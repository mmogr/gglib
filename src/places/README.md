# places

<!-- module-docs:start -->

The main window's places: each is what the address names beside the
sidebar, a route of `app/router`. A place arranges and formats what the
registries and routes say; every fact and word it shows is the daemon's.

| Folder | Place |
|--------|-------|
| `library/` | The library, home: `/library`, `/models/:id`, `/far/:id`, with the inspector beside it while it has a subject |
| `devices/` | The devices: `/devices`, this Mac shared with your devices, the paired devices, another machine used from here |
| `settings/` | The settings: `/settings/:section`, six sections (General, Endpoint, Models, Tools, Engines, About) whose fields save as they change, with Undo |
| `endpoint/` | The endpoint: `/endpoint`, its address, what is loaded and how, what it serves, how a client connects, who uses it, what gglib did; a model inspected beside it by `?inspect=model:<id>` |
| `chat/` | One chat: `/chat/:id`, `/chat/new`, `/chat/far/:id`; the sidebar lists the chats and navigates |

<!-- module-docs:end -->
