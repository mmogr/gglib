//! A loopback port that refuses every connection while a test holds it.
//!
//! A test of what happens when nothing answers needs a port nothing listens
//! on. A port named in the test (`59999`) is not one: on Windows and macOS
//! the system hands out 49152 to 65535 to any listener that asks for port 0,
//! on Linux 32768 to 60999, and another test's fake server in the same run
//! can be given the very port. The request then gets that server's answer
//! where the test wanted a refusal. A port bound and let go is free the same
//! way from the moment it is let go.
//!
//! [`RefusingPort`] keeps the port instead: a socket is bound to it and then
//! connected to a listener of the holder's own. In use, the port is handed
//! to nobody who asks the system for any port, which is how a fake server
//! gets its own; with no listener on it, a connection to it is refused.
//!
//! Both steps are needed, and each was measured over 200,000 connections:
//!
//! - A socket that is bound and nothing more refuses on Linux, but macOS
//!   drops a connection to one without an answer and the caller waits out
//!   its timeout.
//! - A socket that is connected without being bound first takes its port as
//!   an outgoing connection does. On Linux a later connection to that port
//!   can be given the same port for its own end, and then it connects to
//!   itself: 2 in 200,000 did.

use std::io;
use std::net::{Ipv4Addr, SocketAddr, TcpListener};

use socket2::{Domain, Socket, Type};

/// A loopback port that refuses every connection for as long as this lives.
///
/// Keep it until the test has made its request: dropping it frees the port.
#[derive(Debug)]
pub struct RefusingPort {
    /// Never accepted from: it is only somewhere for `_near` to connect.
    _far: TcpListener,
    /// The socket bound to the port held, and connected to `_far`.
    _near: Socket,
    port: u16,
}

impl RefusingPort {
    /// Take a loopback port from the system and hold it, refusing.
    ///
    /// # Errors
    ///
    /// When the system has no socket or no port to give.
    pub fn hold() -> io::Result<Self> {
        let far = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
        let near = Socket::new(Domain::IPV4, Type::STREAM, None)?;
        near.bind(&SocketAddr::from((Ipv4Addr::LOCALHOST, 0)).into())?;
        near.connect(&far.local_addr()?.into())?;
        let port = near
            .local_addr()?
            .as_socket()
            .map(|address| address.port())
            .ok_or_else(|| io::Error::other("the held socket has no port"))?;
        Ok(Self {
            _far: far,
            _near: near,
            port,
        })
    }

    /// The port held.
    #[must_use]
    pub const fn port(&self) -> u16 {
        self.port
    }
}

#[cfg(test)]
#[path = "refusing_tests.rs"]
mod tests;
