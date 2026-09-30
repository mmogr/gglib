//! The daemon's own credential, on disk.
//!
//! A loopback daemon's socket is the machine's and not the owner's: any
//! account on it can reach `127.0.0.1:9887`, and through `/api` pair a device,
//! run a command as an MCP server, or rewrite settings. So every `/api` route
//! asks for this token. `proxy_api_key` cannot serve: it is the proxy's key,
//! handed to the proxy's clients and printed by `gglib config settings show`.
//!
//! The file is `0600` beside the device keys, written the way they are, so
//! reading it is proof of being the owner's account on this machine (or
//! root). **A new one at every start.** Whatever bound the port while the
//! daemon was down, another account's program included, could answer
//! `/health` as the daemon and read the token a client sent it; minting again
//! at start makes that token worthless from then on. Clients read the file at
//! every call, so only an open page needs a fresh link after a restart.

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::bearer_matches;
use super::private_file::write_private;
use crate::paths::{PathError, remote_identity_path};

/// How many random bytes a token carries: 256 bits, written as hex.
const TOKEN_BYTES: usize = 32;

/// The daemon's token. Its `Debug` hides the value, so logging a struct that
/// holds one does not print it.
#[derive(Clone, PartialEq, Eq)]
pub struct DaemonToken(Arc<str>);

impl DaemonToken {
    /// A new token from the OS's random source.
    ///
    /// # Errors
    ///
    /// [`io::Error`] when the OS has no randomness to give.
    pub fn mint() -> io::Result<Self> {
        let mut bytes = [0u8; TOKEN_BYTES];
        getrandom::fill(&mut bytes).map_err(io::Error::other)?;
        let hex: String = bytes
            .iter()
            .flat_map(|b| [b >> 4, b & 0xf])
            .map(|nibble| char::from_digit(u32::from(nibble), 16).unwrap_or('0'))
            .collect();
        Ok(Self(Arc::from(hex)))
    }

    /// Whether `presented`, the raw `Authorization` header or `None` when the
    /// client sent none, carries this token. [`bearer_matches`] decides, so
    /// the scheme and the constant-time comparison are the API key's.
    #[must_use]
    pub fn admits(&self, presented: Option<&str>) -> bool {
        bearer_matches(presented, &self.0)
    }

    /// The token itself, for a client to send.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for DaemonToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("DaemonToken(..)")
    }
}

/// Where the token lives: beside the device keys, in `data/`.
///
/// # Errors
///
/// Whatever resolving the data root returns.
pub fn daemon_token_path() -> Result<PathBuf, PathError> {
    Ok(remote_identity_path()?.with_file_name("daemon_token"))
}

/// The stored token, or `None` when there is none yet. What a client calls:
/// it never mints, because a token the daemon did not mint is one it refuses.
///
/// A blank file is no token, as a blank `proxy_api_key` is no key.
///
/// # Errors
///
/// [`io::Error`] when the file exists and cannot be read, as it cannot by
/// another account, and when group or other may read or write it: a token
/// others could read, or write one of their own into, proves nothing.
pub fn read(path: &Path) -> io::Result<Option<DaemonToken>> {
    refuse_loose(path)?;
    match fs::read_to_string(path) {
        Ok(text) => {
            let token = text.trim();
            Ok((!token.is_empty()).then(|| DaemonToken(Arc::from(token))))
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}

/// A new token, written over whatever the file held. What the daemon calls at
/// every start, holding the daemon lock, so no two mint at once.
///
/// Written through [`write_private`]: a new `0600` file renamed into place, so
/// a file somebody left there, loose or not, is replaced rather than reused.
///
/// # Errors
///
/// [`io::Error`] from minting, or from writing the file.
pub fn mint_and_store(path: &Path) -> io::Result<DaemonToken> {
    let token = DaemonToken::mint()?;
    write_private(path, token.as_str().as_bytes())?;
    Ok(token)
}

/// Refuse a file group or other may read or write. Absent is not refused.
#[cfg(unix)]
fn refuse_loose(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    match fs::metadata(path) {
        Ok(meta) if meta.permissions().mode() & 0o077 != 0 => Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!("{} is open to other accounts", path.display()),
        )),
        Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}

/// Windows has no mode to read: the file has the directory's ACL, so nothing
/// is refused. The signature matches the Unix one for the callers' sake.
#[cfg(not(unix))]
#[allow(
    clippy::unnecessary_wraps,
    reason = "the Unix twin returns io::Result; the callers share one signature"
)]
fn refuse_loose(_path: &Path) -> io::Result<()> {
    Ok(())
}

#[cfg(test)]
#[path = "daemon_token_tests.rs"]
mod daemon_token_tests;
