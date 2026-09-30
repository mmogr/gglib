//! The daemon's own credential, on disk.
//!
//! A loopback daemon asks no API key, because the socket is the boundary. But
//! the socket is the machine's and not the owner's: any account on it can
//! reach `127.0.0.1:9887`. Most routes do little harm in other hands. The ones
//! that change who is trusted do: enabling the tunnel and inviting a device
//! hands whoever asked a key that outlives them. Those routes ask for this
//! token. `proxy_api_key` cannot serve, because `GET /api/config/settings`
//! returns it unmasked to anybody who asks.
//!
//! The file is `0600` beside the device keys, written the way they are, so
//! reading it is proof of being the owner's account on this machine (or
//! root). The daemon mints it the first time it starts and reads it again at
//! every start after, so a client that read it once keeps a token that works.

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
/// it never mints, because a token the daemon did not read is one it refuses.
///
/// A blank file is no token, as a blank `proxy_api_key` is no key.
///
/// # Errors
///
/// [`io::Error`] when the file exists and cannot be read, as it cannot by
/// another account.
pub fn read(path: &Path) -> io::Result<Option<DaemonToken>> {
    match fs::read_to_string(path) {
        Ok(text) => {
            let token = text.trim();
            Ok((!token.is_empty()).then(|| DaemonToken(Arc::from(token))))
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}

/// The stored token, minted and written first when there is none. What the
/// daemon calls at start, holding the daemon lock, so no two mint at once.
///
/// # Errors
///
/// [`io::Error`] from reading the file, or from minting and writing one.
pub fn load_or_mint(path: &Path) -> io::Result<DaemonToken> {
    if let Some(token) = read(path)? {
        return Ok(token);
    }
    let token = DaemonToken::mint()?;
    write_private(path, token.as_str().as_bytes())?;
    Ok(token)
}

#[cfg(test)]
#[path = "daemon_token_tests.rs"]
mod daemon_token_tests;
