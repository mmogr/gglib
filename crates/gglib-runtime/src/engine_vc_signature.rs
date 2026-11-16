//! Whether a downloaded Visual C++ runtime installer is Microsoft's.
//!
//! Microsoft's permalink follows its newest release, so no digest can be
//! pinned. The download is run only when Windows says its Authenticode
//! signature is valid (its chain ends at a root this machine trusts, and its
//! hash is the file's), the signing certificate's organisation is exactly
//! Microsoft Corporation, and so is the organisation of the root its chain
//! ends at. Windows is asked through Windows PowerShell 5.1, which every
//! Windows 10 and 11 has: `Get-AuthenticodeSignature`, which asks
//! `WinVerifyTrust`, gives the status and the signing certificate; the .NET
//! `X509Chain` builds that certificate's chain to its root. No `unsafe` call
//! is added to gglib: its Windows calls stay those ADR 0030 lists.
//!
//! Each certificate's subject comes back as Windows writes a name
//! (`CertNameToStr`, the form .NET's `Subject` gives): relative names
//! separated by commas, the values of a multi-valued one by `+`, and a value
//! holding any of those in double quotes, a quote in it doubled.
//! [`organisation`] reads that form, so a value that merely contains
//! `O=Microsoft Corporation` is not taken for it. The path checked reaches
//! PowerShell in its environment, never in the script's text. A `#[path]`
//! child of `engine_vc_runtime.rs`.

use std::path::Path;
use std::time::Duration;

use serde::Deserialize;

use gglib_core::utils::process::bounded::run_bounded;
use gglib_core::utils::process::cmd;

/// How long Windows has to say who signed the download.
const VERIFY_WAIT: Duration = Duration::from_mins(1);

/// The organisation of a genuine installer's signer and of its root.
const MICROSOFT: &str = "Microsoft Corporation";

/// The variable the path checked is handed to PowerShell in.
const PATH_VARIABLE: &str = "GGLIB_VERIFY_PATH";

/// What PowerShell runs: the signature's status, its signer's subject, and
/// the subject of the root the signer's chain ends at, as one line of JSON
/// in UTF-8. Revocation was already asked by the status, so the chain is
/// built only to find its root.
const SCRIPT: &str = "[Console]::OutputEncoding = [System.Text.Encoding]::UTF8; \
     $s = Get-AuthenticodeSignature -LiteralPath $env:GGLIB_VERIFY_PATH; \
     $signer = $s.SignerCertificate; $root = $null; \
     if ($signer) { \
       $chain = New-Object System.Security.Cryptography.X509Certificates.X509Chain; \
       $chain.ChainPolicy.RevocationMode = 'NoCheck'; \
       [void]$chain.Build($signer); \
       $root = $chain.ChainElements[$chain.ChainElements.Count - 1].Certificate.Subject \
     }; \
     $name = $null; if ($signer) { $name = $signer.Subject }; \
     @{ status = $s.Status.ToString(); signer = $name; root = $root } | ConvertTo-Json -Compress";

/// What Windows says of a file's Authenticode signature.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub(crate) struct Signature {
    /// `Valid`, or why not: `NotSigned`, `HashMismatch`, `NotTrusted`, ….
    pub status: String,
    /// The signing certificate's subject, when there is one.
    pub signer: Option<String>,
    /// The subject of the root certificate the signer's chain ends at.
    pub root: Option<String>,
}

/// Whether the file at `path` may be run: Windows says its signature is
/// valid, and Microsoft signed it under one of its own roots.
pub(crate) fn signed_by_microsoft(path: &Path) -> Result<(), String> {
    microsoft_signed(&signature_of(path)?)
}

/// Whether `signature` is a valid one of Microsoft's, or why not.
pub(crate) fn microsoft_signed(signature: &Signature) -> Result<(), String> {
    if signature.status != "Valid" {
        return Err(format!(
            "Windows says its signature is not valid ({})",
            signature.status
        ));
    }
    for (whose, subject) in [("signer", &signature.signer), ("root", &signature.root)] {
        let Some(subject) = subject else {
            return Err(format!("Windows names no {whose} for its signature"));
        };
        if organisation(subject)?.as_deref() != Some(MICROSOFT) {
            return Err(format!("its {whose} is {subject}, not Microsoft"));
        }
    }
    Ok(())
}

/// The organisation (`O`) a distinguished name names, in the form Windows
/// writes one: `None` when it names none.
///
/// # Errors
///
/// It is not a name in that form, it names more than one organisation, or
/// its organisation shares a relative name with another attribute.
pub(crate) fn organisation(name: &str) -> Result<Option<String>, String> {
    let mut found = None;
    for relative in relative_names(name)? {
        let single = relative.len() == 1;
        for (attribute, value) in relative {
            if !(attribute.eq_ignore_ascii_case("O") || attribute == "2.5.4.10") {
                continue;
            }
            if !single || found.is_some() {
                return Err(format!("{name} does not name one organisation plainly"));
            }
            found = Some(value);
        }
    }
    Ok(found)
}

/// One relative name: its attributes and their values.
type Relative = Vec<(String, String)>;

/// The relative names of `name`, each its attributes and their values,
/// with quotes and escapes taken off the values.
pub(crate) fn relative_names(name: &str) -> Result<Vec<Relative>, String> {
    let malformed = || format!("{name} is not a name Windows writes");
    let mut chars = name.chars().peekable();
    let mut names = Vec::new();
    let mut current: Relative = Vec::new();
    loop {
        while chars.peek().is_some_and(|c| c.is_whitespace()) {
            chars.next();
        }
        if chars.peek().is_none() {
            break;
        }
        let mut attribute = String::new();
        loop {
            match chars.next() {
                Some('=') => break,
                Some(',' | ';' | '+' | '"') | None => return Err(malformed()),
                Some(c) => attribute.push(c),
            }
        }
        while chars.peek().is_some_and(|c| *c == ' ') {
            chars.next();
        }
        let mut value = String::new();
        if chars.peek() == Some(&'"') {
            chars.next();
            loop {
                match chars.next() {
                    Some('"') if chars.peek() == Some(&'"') => {
                        chars.next();
                        value.push('"');
                    }
                    Some('"') => break,
                    Some(c) => value.push(c),
                    None => return Err(malformed()),
                }
            }
            while chars.peek().is_some_and(|c| *c == ' ') {
                chars.next();
            }
        } else {
            while let Some(&c) = chars.peek() {
                if matches!(c, ',' | ';' | '+') {
                    break;
                }
                chars.next();
                if c == '\\' {
                    value.push(chars.next().ok_or_else(malformed)?);
                } else {
                    value.push(c);
                }
            }
            value.truncate(value.trim_end().len());
        }
        current.push((attribute.trim().to_owned(), value));
        match chars.next() {
            Some('+') => {}
            Some(',' | ';') | None => names.push(std::mem::take(&mut current)),
            Some(_) => return Err(malformed()),
        }
    }
    if !current.is_empty() {
        names.push(current);
    }
    Ok(names)
}

/// Ask Windows what it says of `path`'s signature.
fn signature_of(path: &Path) -> Result<Signature, String> {
    let Some(root) = std::env::var_os("SystemRoot") else {
        return Err(
            "Windows' folder is not known, so its signature could not be checked".to_owned(),
        );
    };
    let powershell = Path::new(&root)
        .join("System32")
        .join("WindowsPowerShell")
        .join("v1.0")
        .join("powershell.exe");
    let run = run_bounded(
        cmd(powershell)
            .args(["-NoProfile", "-NonInteractive", "-Command", SCRIPT])
            .env(PATH_VARIABLE, path),
        VERIFY_WAIT,
    )
    .map_err(|e| format!("Windows could not check its signature ({e})"))?;
    if !run.status.success() {
        return Err(format!(
            "Windows could not check its signature ({})",
            String::from_utf8_lossy(&run.stderr).trim()
        ));
    }
    parse_signature(&String::from_utf8_lossy(&run.stdout))
}

/// [`signature_of`]'s answer from what the script printed.
pub(crate) fn parse_signature(printed: &str) -> Result<Signature, String> {
    let printed = printed.trim_start_matches('\u{feff}').trim();
    serde_json::from_str(printed)
        .map_err(|e| format!("Windows' answer about its signature could not be read ({e})"))
}

#[cfg(test)]
#[path = "engine_vc_signature_tests.rs"]
mod tests;
