//! The daemon's loopback port, and the one environment variable that moves it.

use super::DEFAULT_DAEMON_PORT;

/// The loopback port of the gglib daemon's management API.
///
/// Not a setting: the daemon is the one process every client (CLI, desktop
/// app, browser dashboard) must find without configuration, and a port kept
/// in the database would reopen the "two daemons on different ports"
/// split-brain. It is [`DEFAULT_DAEMON_PORT`] unless the process's
/// environment sets `GGLIB_DAEMON_PORT`, which goes with `GGLIB_DATA_DIR`:
/// a development build given a data root of its own also needs a daemon of
/// its own, or its clients reach the daemon of the other root and are
/// refused, since the daemon token lives in the data root.
///
/// A value that is not a port from 1 to 65535 is reported once on stderr and
/// the default is used.
#[must_use]
pub fn daemon_port() -> u16 {
    static PORT: std::sync::OnceLock<u16> = std::sync::OnceLock::new();
    *PORT.get_or_init(|| parse_daemon_port(std::env::var("GGLIB_DAEMON_PORT").ok().as_deref()))
}

/// [`daemon_port`]'s reading of a `GGLIB_DAEMON_PORT` value.
fn parse_daemon_port(value: Option<&str>) -> u16 {
    let Some(value) = value else {
        return DEFAULT_DAEMON_PORT;
    };
    match value.trim().parse::<u16>() {
        Ok(port) if port != 0 => port,
        _ => {
            eprintln!(
                "warning: GGLIB_DAEMON_PORT={value:?} is not a port from 1 to 65535; using {DEFAULT_DAEMON_PORT}"
            );
            DEFAULT_DAEMON_PORT
        }
    }
}

#[cfg(test)]
mod daemon_port_tests {
    use super::parse_daemon_port;
    use crate::settings::DEFAULT_DAEMON_PORT;

    #[test]
    fn unset_is_the_default() {
        assert_eq!(parse_daemon_port(None), DEFAULT_DAEMON_PORT);
    }

    #[test]
    fn a_port_is_taken() {
        assert_eq!(parse_daemon_port(Some("9888")), 9888);
        assert_eq!(parse_daemon_port(Some(" 9888 ")), 9888);
    }

    #[test]
    fn anything_else_is_the_default() {
        for value in ["", "0", "65536", "-1", "port"] {
            assert_eq!(
                parse_daemon_port(Some(value)),
                DEFAULT_DAEMON_PORT,
                "{value}"
            );
        }
    }
}
