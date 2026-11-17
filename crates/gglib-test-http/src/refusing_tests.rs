//! What a held port promises: a connection to it is refused, the system
//! hands it to nobody who asks for any port, and two holders never hold the
//! same one.

use std::io::ErrorKind;
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};

use socket2::{Domain, Socket, Type};

use super::RefusingPort;

#[test]
fn a_connection_to_a_held_port_is_refused() {
    let held = RefusingPort::hold().unwrap();

    let refused = TcpStream::connect((Ipv4Addr::LOCALHOST, held.port())).unwrap_err();

    assert_eq!(refused.kind(), ErrorKind::ConnectionRefused, "{refused}");
}

/// More than one is refused. The claim that every one is was measured by
/// hand, 200,000 connections on a Mac and on Linux (the module's docs): on
/// Linux a port held only as an outgoing connection's own end let 2 of
/// them connect to themselves. A run that long does not belong in a test:
/// 100,000 sockets in one ran a CI Mac out of buffers ("No buffer space
/// available"). Windows takes about two seconds to say a connection is
/// refused, so it asks twice.
#[test]
fn connection_after_connection_to_a_held_port_is_refused() {
    let held = RefusingPort::hold().unwrap();
    let asks = if cfg!(windows) { 2 } else { 500 };

    for _ in 0..asks {
        let refused = TcpStream::connect((Ipv4Addr::LOCALHOST, held.port()));
        assert!(refused.is_err(), "a connection to a held port was made");
    }
}

/// A fake server asks for port 0 and takes what the system gives, with the
/// address reusable (as `std` asks on Unix) or not. None of these is given
/// the held port. That none ever is was measured by hand over 200,000 asks,
/// where a port bound and let go came back 12 times on a Mac and 36 times
/// on Linux; this many only shows the two kinds of ask both work.
#[test]
fn a_held_port_is_handed_to_nobody_who_asks_for_any() {
    let held = RefusingPort::hold().unwrap();
    let any = SocketAddr::from((Ipv4Addr::LOCALHOST, 0));

    for _ in 0..500 {
        let listener = TcpListener::bind(any).unwrap();
        assert_ne!(listener.local_addr().unwrap().port(), held.port());

        let plain = Socket::new(Domain::IPV4, Type::STREAM, None).unwrap();
        plain.bind(&any.into()).unwrap();
        let given = plain.local_addr().unwrap().as_socket().unwrap().port();
        assert_ne!(given, held.port());
    }
}

#[test]
fn each_holder_has_a_port_of_its_own() {
    let held: Vec<RefusingPort> = (0..32).map(|_| RefusingPort::hold().unwrap()).collect();

    let mut ports: Vec<u16> = held.iter().map(RefusingPort::port).collect();
    ports.sort_unstable();
    ports.dedup();

    assert_eq!(ports.len(), held.len());
}
