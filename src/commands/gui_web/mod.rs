#![doc = include_str!(concat!(env!("OUT_DIR"), "/commands_gui_web_docs.md"))]

pub mod handlers;
pub mod routes;
pub mod server;
pub mod state;

pub use server::start_web_server;
