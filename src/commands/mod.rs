#![doc = include_str!(concat!(env!("OUT_DIR"), "/commands_docs.md"))]

// add: Migrated to gglib-cli/src/handlers/add.rs (#180)
pub mod assistant_ui;
// chat: Migrated to gglib-cli/src/handlers/chat.rs (#180)
pub mod check_deps;
// config: Migrated to gglib-cli/src/handlers/config.rs (#180)
pub mod download;
pub mod gui_web;
// list: Migrated to gglib-cli/src/handlers/list.rs (#180)
pub mod llama;
pub mod llama_args;
pub mod llama_invocation;
pub mod presentation;
// remove: Migrated to gglib-cli/src/handlers/remove.rs (#180)
// serve: Migrated to gglib-cli/src/handlers/serve.rs (#180)
// update: Migrated to gglib-cli/src/handlers/update.rs (#180)
