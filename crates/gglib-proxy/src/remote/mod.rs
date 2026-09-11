#![doc = include_str!("README.md")]

mod device_gate;
mod marker;
mod mcp_guard;
mod pair;

pub(crate) use device_gate::device_gate;
pub(crate) use marker::{Tunnelled, remote_marker};
pub(crate) use mcp_guard::mcp_tunnel_guard;
pub(crate) use pair::handle_remote_pair;
