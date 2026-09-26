//! The stored endpoint key: where it lives, and clearing one that holds no
//! key so the daemon can arm.
//!
//! Not `key.rs`, which is about the credential the *proxy* enforces. Both
//! are called keys and are otherwise unrelated: this one is modelpipe's
//! endpoint identity, the file every paired device knows this machine by,
//! and nothing here reads settings or the proxy.

use tracing::info;

use crate::error::GuiError;

/// Where this machine's endpoint key lives.
///
/// Always the same file (ADR 0012, decision 4, as reversed). A ticket names
/// a machine; it is not a credential, and reaching anything behind it still
/// takes a key this side issued. A key minted every session would change the
/// address for a reason nobody outside this process could see, and every
/// paired device would pair again after a reboot.
///
/// Decision 3's arithmetic — six digits and two minutes are enough only while
/// a guesser has to find the listener first — does not hold for a lasting
/// ticket, so the guesses are counted where they arrive: the tunnel edge
/// answers the code itself and counts wrong codes per endpoint. Retiring the
/// address is deleting this file, a thing a person does deliberately
/// rather than a side effect of a restart. It revokes no device: `arm` seeds
/// every device key the roster lists onto the listener at the new address,
/// so cutting a device off is [`RemoteOps::forget`](super::RemoteOps::forget).
///
/// Separate from `arm` so the decision can be read without binding an
/// endpoint or writing a key: `arm` is a network call and a file, and this
/// is neither.
pub(super) fn identity_path() -> Result<Option<std::path::PathBuf>, GuiError> {
    gglib_core::paths::remote_identity_path()
        .map(Some)
        .map_err(|e| GuiError::Internal(format!("could not place the stored endpoint key: {e}")))
}

/// Discard a stored endpoint key with nothing in it, so the daemon can arm.
///
/// modelpipe refuses an empty identity file rather than minting over it, and
/// the refusal is permanent and deliberate: a file that exists may be a key
/// devices are already paired against, and replacing it would unpair them
/// silently. modelpipe 0.7 does not *produce* an empty one — its writes go
/// through a temporary — but a modelpipe before 0.7 wrote in place, where a
/// crash between the open and the bytes leaves exactly this, and the file
/// outlives the upgrade.
///
/// Without the heal an affected desktop could never arm again, and the
/// remedy would reach its operator only as a sentence inside a failure they
/// have no reason to go looking for.
///
/// **The predicate is modelpipe's, not "zero bytes".** Over there the test
/// is `trim().is_empty()`, so a file holding only a newline gets the same
/// "it is empty" refusal as an empty one; matching that is what makes this
/// heal exactly the state it exists for rather than most of it. Bytes that
/// are not UTF-8 are left alone — they are wrong, not absent.
///
/// A key modelpipe wrote is 53 bytes, so the read costs nothing — but this
/// function exists for a path that may hold something modelpipe did not
/// write, and nothing bounds what is there. Anything over a kilobyte is left
/// for modelpipe to refuse rather than read into memory to find out.
///
/// **Empty is the only state healed.** A file with a real key in it, or a
/// corrupt one, may still be what this machine is paired under, and deleting
/// that would unpair every device to make one enable succeed. It keeps
/// modelpipe's refusal, which is the operator's to act on.
///
/// **Safe to unlink here although modelpipe declines to.** Upstream will not
/// remove a file it did not write, because two listeners recovering at once
/// would race and the second could delete the valid key the first had just
/// minted. gglib has no second recoverer: the daemon lock admits one daemon
/// per machine, and `enable` holds the serve slot's reservation across this
/// call.
///
/// A symlink is followed by the stat and unlinked by the removal, so a link
/// pointing at an empty file loses the link rather than the target. That is
/// the outcome wanted — the target held no key either way.
///
/// Absent is the ordinary first-enable case and is not an error, and neither
/// is the file going away underneath this: both leave the path with no empty
/// key at it, which is all this promises.
///
/// The `is_file` guard is redundant with the read and kept for what it says:
/// dropping it on its own changes nothing, because reading a directory fails
/// and falls through the same way.
pub(super) fn discard_empty_identity(path: &std::path::Path) -> Result<(), GuiError> {
    let Ok(meta) = std::fs::metadata(path) else {
        return Ok(());
    };
    if !meta.is_file() {
        return Ok(());
    }
    if meta.len() > 1024 {
        return Ok(());
    }
    let Ok(text) = std::fs::read_to_string(path) else {
        return Ok(());
    };
    if !text.trim().is_empty() {
        return Ok(());
    }
    match std::fs::remove_file(path) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => {
            return Err(GuiError::Internal(format!(
                "the stored endpoint key at {} holds no key and could not be removed: {e}",
                path.display()
            )));
        }
    }
    info!("discarded an empty stored endpoint key; a new one will be minted");
    Ok(())
}

#[cfg(test)]
#[path = "identity_tests.rs"]
mod identity_tests;
