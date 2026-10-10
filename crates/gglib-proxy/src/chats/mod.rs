#![doc = include_str!("README.md")]

mod attachments;
mod guard;
mod handlers;

pub(crate) use attachments::{body_limit, fetch_attachment, upload_attachment};
pub(crate) use guard::named_device_only;
pub(crate) use handlers::{change_chat, list_chats, open_chat};
