//! Ask the installed llama-server what it is.
//!
//! The counterpart to GGUF capability detection: that reads a *model* file and
//! produces [`GgufCapabilities`]; this runs the *binary* and produces
//! [`RuntimeCapabilities`]. Both answer the same shape of question — what can
//! this thing do, and therefore what should gglib do differently — and both
//! must answer it before a launch, not during a request.
//!
//! # Why a cache
//!
//! A binary's version cannot change while it sits on disk, but a probe costs a
//! process spawn, and the launch path is already latency-sensitive enough that
//! [`admission`] exists to keep swaps off the critical path. The result is
//! memoized per binary path so repeat launches — the common case, since one
//! binary serves every model — pay for it once per process.
//!
//! Keying on the path rather than a global slot is what keeps a source build
//! under test and an installed release from being mistaken for each other when
//! both are exercised in one process.
//!
//! # Failure is a result, not an error
//!
//! Every failure mode — binary missing, non-zero exit, unreadable banner —
//! produces [`RuntimeCapabilities::unknown`] rather than an `Err`. A probe
//! exists to *inform* compensation decisions, and the answer to "we could not
//! tell" is already well-defined: compensate. Returning an error would make
//! every caller re-derive that policy, and would let a probe failure block a
//! launch that would otherwise have worked exactly as it always has.
//!
//! [`GgufCapabilities`]: gglib_core::GgufCapabilities
//! [`admission`]: crate::process::admission

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use gglib_core::domain::RuntimeCapabilities;
use gglib_core::utils::process::cmd;
use tracing::{debug, warn};

/// Memoized probe results, keyed by binary path.
///
/// `std::sync::Mutex` around a small map, matching the lock discipline used
/// elsewhere for this shape of cache: every critical section is a couple of
/// map operations with no `.await` inside.
static PROBE_CACHE: Mutex<Option<HashMap<PathBuf, RuntimeCapabilities>>> = Mutex::new(None);

/// Probe `binary` for its version, memoizing the result per path.
///
/// Never fails: an unprobeable binary yields [`RuntimeCapabilities::unknown`],
/// which claims no capabilities and therefore leaves every compensation on.
pub fn probe(binary: &Path) -> RuntimeCapabilities {
    if let Some(hit) = cached(binary) {
        return hit;
    }

    let caps = probe_uncached(binary);

    if caps.is_identified() {
        debug!(
            binary = %binary.display(),
            build = ?caps.build,
            flags = ?caps.flags,
            "probed llama-server capabilities"
        );
    } else {
        // Worth a warning rather than a debug: an unidentified runtime silently
        // costs the user every native fast path gglib might otherwise defer to.
        warn!(
            binary = %binary.display(),
            version_line = %caps.version_line,
            "could not identify llama-server build; assuming no native capabilities"
        );
    }

    store(binary, &caps);
    caps
}

/// Run the binary and interpret its banner, with no cache involvement.
fn probe_uncached(binary: &Path) -> RuntimeCapabilities {
    if !binary.exists() {
        return RuntimeCapabilities::unknown("binary not found");
    }

    let output = match cmd(binary).arg("--version").output() {
        Ok(output) => output,
        Err(e) => return RuntimeCapabilities::unknown(format!("probe failed: {e}")),
    };

    if !output.status.success() {
        return RuntimeCapabilities::unknown(format!("probe exited with status {}", output.status));
    }

    // llama.cpp writes its version banner to stderr and has historically moved
    // between the two, so both are read. stderr first: when a build prints to
    // both, the banner is the one on stderr and stdout carries help text.
    let stderr = String::from_utf8_lossy(&output.stderr);
    let caps = RuntimeCapabilities::from_version_output(&stderr);
    if caps.is_identified() {
        return caps;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let caps = RuntimeCapabilities::from_version_output(&stdout);
    if caps.is_identified() {
        return caps;
    }

    // Neither stream parsed. Prefer a non-empty banner for the record so the
    // stored value shows what the binary actually said.
    if stderr.trim().is_empty() {
        RuntimeCapabilities::from_version_output(&stdout)
    } else {
        RuntimeCapabilities::from_version_output(&stderr)
    }
}

/// The cached result for `binary`, if one has been taken.
fn cached(binary: &Path) -> Option<RuntimeCapabilities> {
    PROBE_CACHE.lock().ok()?.as_ref()?.get(binary).cloned()
}

/// Memoize `caps` for `binary`.
fn store(binary: &Path, caps: &RuntimeCapabilities) {
    if let Ok(mut guard) = PROBE_CACHE.lock() {
        guard
            .get_or_insert_with(HashMap::new)
            .insert(binary.to_path_buf(), caps.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gglib_core::domain::RuntimeFlags;

    /// The policy this module exists to guarantee: a binary that is not there
    /// claims nothing, so callers keep compensating.
    #[test]
    fn a_missing_binary_probes_as_unidentified() {
        let caps = probe_uncached(Path::new("/nonexistent/llama-server"));

        assert!(!caps.is_identified());
        assert!(!caps.has(RuntimeFlags::PEG_NATIVE_TOOL_CALLS));
        assert_eq!(caps.version_line, "binary not found");
    }

    /// A probe must never panic on a path that exists but is not a runnable
    /// llama-server — a directory is the cheapest such path to construct.
    #[test]
    fn a_non_executable_path_probes_as_unidentified() {
        let dir = tempfile::tempdir().expect("tempdir");
        let caps = probe_uncached(dir.path());

        assert!(!caps.is_identified());
    }

    /// Caching must round-trip through the real `probe` entry point, and a
    /// second call must not change the answer.
    #[test]
    fn repeat_probes_agree() {
        let path = Path::new("/nonexistent/llama-server-cache-test");

        let first = probe(path);
        let second = probe(path);

        assert_eq!(first, second);
        assert!(!second.is_identified());
    }
}
