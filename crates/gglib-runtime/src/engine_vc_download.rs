//! Downloading the Visual C++ runtime's installer from Microsoft's
//! permalink into the file made for it.
//!
//! Trust rests on the installer's signature, checked before it runs
//! (`engine_vc_signature.rs`), not on where it came from: Microsoft's
//! permalink redirects to wherever its current release is. What is bounded
//! here is what a download can cost: every hop is HTTPS, and the body is
//! written to the file as it arrives, refused past [`CAP`] (the real
//! installer is about 25 MB) instead of being held in memory. A `#[path]`
//! child of `engine_vc_runtime.rs`.

use std::fmt::Display;
use std::time::Duration;

use futures_util::{Stream, StreamExt as _};
use tokio::fs::File;
use tokio::io::{AsyncWrite, AsyncWriteExt as _};

use gglib_core::utils::process::bounded::VISUAL_CPP_RUNTIME_X64_URL;

use super::Digest;

/// The most a download may be. Far above the real installer's size, far
/// below what would trouble the service's memory or the disk.
pub(crate) const CAP: u64 = 200 * 1024 * 1024;

/// How long the download from Microsoft has.
const DOWNLOAD_WAIT: Duration = Duration::from_mins(10);

/// The most redirects followed: ten, and the eleventh is refused.
pub(crate) const MOST_HOPS: usize = 10;

/// Whether a redirect is one too many, given how many addresses its chain
/// has asked already (`previous`, the first one among them): the redirect
/// asked now is redirect number `previous`.
pub(crate) const fn too_many_hops(previous: usize) -> bool {
    previous > MOST_HOPS
}

/// Download Microsoft's current installer into `file`, just created at the
/// path given (which is not needed: the file is written through its
/// handle), and give the SHA-256 of what was written.
pub(crate) async fn download_from_microsoft(
    _path: std::path::PathBuf,
    mut file: File,
) -> Result<Digest, String> {
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(30))
        .timeout(DOWNLOAD_WAIT)
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if let Some(reason) = refused_hop(attempt.url()) {
                attempt.error(reason)
            } else if too_many_hops(attempt.previous().len()) {
                attempt.error(format!("more than {MOST_HOPS} redirects"))
            } else {
                attempt.follow()
            }
        }))
        .build()
        .map_err(|e| e.to_string())?;
    let url = reqwest::Url::parse(VISUAL_CPP_RUNTIME_X64_URL).map_err(|e| e.to_string())?;
    if let Some(reason) = refused_hop(&url) {
        return Err(reason);
    }
    let response = client
        .get(url)
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(|e| e.to_string())?;
    if let Some(length) = response.content_length()
        && length > CAP
    {
        return Err(too_large());
    }
    let (_, digest) = copy_capped(response.bytes_stream(), &mut file, CAP).await?;
    file.sync_all().await.map_err(|e| e.to_string())?;
    Ok(digest)
}

/// Why a download, or a redirect it was sent on, is not followed: it is
/// not HTTPS.
pub(crate) fn refused_hop(url: &reqwest::Url) -> Option<String> {
    (url.scheme() != "https").then(|| format!("{url} is not HTTPS, so it was not followed"))
}

/// Write `body` to `to` as it arrives, refusing it once it passes `cap`
/// bytes; the bytes written, and their SHA-256.
pub(crate) async fn copy_capped<B, E>(
    body: impl Stream<Item = Result<B, E>>,
    to: &mut (impl AsyncWrite + Unpin),
    cap: u64,
) -> Result<(u64, Digest), String>
where
    B: AsRef<[u8]>,
    E: Display,
{
    use sha2::Digest as _;
    let mut body = std::pin::pin!(body);
    let mut written: u64 = 0;
    let mut hasher = sha2::Sha256::new();
    while let Some(chunk) = body.next().await {
        let chunk = chunk.map_err(|e| e.to_string())?;
        let bytes = chunk.as_ref();
        written += bytes.len() as u64;
        if written > cap {
            return Err(too_large());
        }
        to.write_all(bytes).await.map_err(|e| e.to_string())?;
        hasher.update(bytes);
    }
    to.flush().await.map_err(|e| e.to_string())?;
    Ok((written, hasher.finalize().into()))
}

fn too_large() -> String {
    format!(
        "it is larger than {} MB, far more than the installer is, so it was refused",
        CAP / 1024 / 1024
    )
}

#[cfg(test)]
#[path = "engine_vc_download_tests.rs"]
mod tests;
