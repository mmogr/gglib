#![doc = include_str!("README.md")]

mod guard;
mod handlers;

pub(crate) use guard::named_device_only;
pub(crate) use handlers::{list_chats, open_chat};
