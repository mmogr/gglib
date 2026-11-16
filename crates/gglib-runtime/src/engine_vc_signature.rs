//! Whether a downloaded file is Microsoft's Visual C++ runtime installer.
//!
//! Microsoft's permalink follows its newest release, so no digest can be
//! pinned. Two checks are made, kept apart so each is tested on its own:
//!
//! - **The signature** ([`microsoft_signed`]): Windows says its
//!   Authenticode signature is valid (its chain ends at a root this machine
//!   trusts, and its hash is the file's); the signing certificate's
//!   organisation is exactly Microsoft Corporation; its chain builds, and
//!   ends at a self-signed root that is one of Microsoft's own two code
//!   signing roots, pinned by thumbprint ([`MICROSOFT_ROOTS`], taken from
//!   Microsoft's PKI repository). A file Microsoft's signing service signed
//!   for a customer chains elsewhere and is refused, as is any root gglib
//!   does not know.
//! - **The product** ([`visual_cpp_installer`]): the version information
//!   the signature covers names the x64 Visual C++ runtime, at 14.30 or
//!   later, so an older or another Microsoft program is not run in its
//!   place.
//!
//! Windows is asked through Windows PowerShell 5.1, which every Windows 10
//! and 11 has: `Get-AuthenticodeSignature` (which asks `WinVerifyTrust`)
//! for the status and the signer, .NET's `X509Chain` for the chain, and
//! .NET's `FileVersionInfo` for the product. No `unsafe` call is added to
//! gglib: its Windows calls stay those ADR 0030 lists. The path checked
//! reaches PowerShell in its environment, never in the script's text.
//!
//! The cmdlet's module is loaded from Windows PowerShell's own folder by its
//! full path, the cmdlet is called by its module-qualified name, and the
//! module path PowerShell searches is set to that folder alone: a module of
//! the same name in the user's Documents folder is not loaded in its place.
//! The script uses no other cmdlet. That does not stop a process of the
//! same user that writes into PowerShell's memory once it runs; nothing
//! gglib does can (see `engine_vc_runtime.rs`).
//!
//! Each certificate's subject comes back as Windows writes a name
//! (`CertNameToStr`, the form .NET's `Subject` gives): relative names
//! separated by `, `, the values of a multi-valued one by ` + `, and a
//! value holding any of those in double quotes, a quote in it doubled.
//! [`organisation`] reads exactly that form and refuses anything else, so
//! a value that merely contains `O=Microsoft Corporation` is not taken for
//! it. A `#[path]` child of `engine_vc_runtime.rs`.

use std::path::Path;
use std::time::Duration;

use base64::Engine as _;

use gglib_core::utils::process::bounded::run_bounded;
use gglib_core::utils::process::cmd;

/// How long Windows has to say who signed the download.
const VERIFY_WAIT: Duration = Duration::from_mins(1);

/// The organisation of a genuine installer's signer.
const MICROSOFT: &str = "Microsoft Corporation";

/// Microsoft's code signing roots, by SHA-1 thumbprint, as Microsoft's PKI
/// repository (<https://www.microsoft.com/pkiops/docs/repository.htm>)
/// lists them and as their certificates from there hash: Microsoft Root
/// Certificate Authority 2011 (`MicRooCerAut2011_2011_03_22.crt`), which
/// the Visual C++ installer chains to, and 2010
/// (`MicRooCerAut_2010-06-23.crt`), which Windows' own programs chain to.
pub(crate) const MICROSOFT_ROOTS: [(&str, &str); 2] = [
    (
        "8F43288AD272F3103B6FB1428485EA3014C0BCFE",
        "Microsoft Root Certificate Authority 2011",
    ),
    (
        "3B1EFD3A66EA28B16697394703A72CA340A05BD5",
        "Microsoft Root Certificate Authority 2010",
    ),
];

/// The names Microsoft has given the x64 runtime's installer
/// (`ProductName`, before ` - <version>`) since Visual C++ 2022: "2015-2022"
/// for 14.3x and 14.4x, and "v14" from 14.50 on (the current download, read
/// from its version information: "Microsoft Visual C++ v14 Redistributable
/// (x64) - 14.52.37029"). The "2015-2019" name is 14.2x, below [`FLOOR`].
const PRODUCT_NAMES: [&str; 2] = [
    "Microsoft Visual C++ v14 Redistributable (x64)",
    "Microsoft Visual C++ 2015-2022 Redistributable (x64)",
];

/// The oldest installer run: 14.30, Visual C++ 2022's first, the first
/// whose runtime has every DLL the engines' builds load.
const FLOOR: (u32, u32) = (14, 30);

/// The variable the path checked is handed to PowerShell in.
const PATH_VARIABLE: &str = "GGLIB_VERIFY_PATH";

/// What a restricted PowerShell answers instead of checking.
const RESTRICTED: &str = "GGLIB-RESTRICTED";

/// Where the answer starts and ends in what PowerShell prints, so a
/// warning printed before it does not spoil it.
const BEGIN: &str = "GGLIB-BEGIN";
const END: &str = "GGLIB-END";

/// What PowerShell runs. Each value is printed as `name=` and its UTF-8 in
/// base64, between [`BEGIN`] and [`END`], with no cmdlet but the one
/// imported from Windows PowerShell's own folder.
const SCRIPT: &str = "$ErrorActionPreference = 'Stop'; \
     if ($ExecutionContext.SessionState.LanguageMode -ne 'FullLanguage') { 'GGLIB-RESTRICTED'; exit 3 }; \
     Import-Module ($PSHOME + '\\Modules\\Microsoft.PowerShell.Security\\Microsoft.PowerShell.Security.psd1'); \
     function e($x) { if ($null -eq $x) { '' } else { [Convert]::ToBase64String([System.Text.Encoding]::UTF8.GetBytes([string]$x)) } }; \
     $p = $env:GGLIB_VERIFY_PATH; \
     $s = Microsoft.PowerShell.Security\\Get-AuthenticodeSignature -LiteralPath $p; \
     $c = $s.SignerCertificate; $built = $false; $root = $null; \
     if ($c) { \
       $chain = [System.Security.Cryptography.X509Certificates.X509Chain]::new(); \
       $chain.ChainPolicy.RevocationMode = [System.Security.Cryptography.X509Certificates.X509RevocationMode]::NoCheck; \
       $built = $chain.Build($c); \
       $root = $chain.ChainElements[$chain.ChainElements.Count - 1].Certificate \
     }; \
     $v = [System.Diagnostics.FileVersionInfo]::GetVersionInfo($p); \
     'GGLIB-BEGIN'; \
     'status=' + (e $s.Status.ToString()); \
     'signer=' + (e $(if ($c) { $c.Subject })); \
     'built=' + (e $built); \
     'root=' + (e $(if ($root) { $root.Subject })); \
     'rootIssuer=' + (e $(if ($root) { $root.Issuer })); \
     'rootThumbprint=' + (e $(if ($root) { $root.Thumbprint })); \
     'product=' + (e $v.ProductName); \
     'major=' + (e $v.FileMajorPart); \
     'minor=' + (e $v.FileMinorPart); \
     'GGLIB-END'";

/// What Windows says of a file's Authenticode signature and its chain.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct Signature {
    /// `Valid`, or why not: `NotSigned`, `HashMismatch`, `NotTrusted`, ….
    pub status: String,
    /// The signing certificate's subject, when there is one.
    pub signer: Option<String>,
    /// Whether the signer's chain built to a trusted root.
    pub chain_built: bool,
    /// The certificate the chain ends at.
    pub root: Option<Root>,
}

/// The certificate a chain ends at.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct Root {
    pub subject: String,
    pub issuer: String,
    /// Its SHA-1 thumbprint, in upper-case hex.
    pub thumbprint: String,
}

/// What a file's version information says it is.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct Product {
    /// `ProductName`.
    pub name: Option<String>,
    /// `FileMajorPart` and `FileMinorPart`.
    pub version: (u32, u32),
}

/// Whether the file at `path` is Microsoft's x64 Visual C++ runtime
/// installer: signed by Microsoft under one of its roots, and the product.
pub(crate) fn installer_checked(path: &Path) -> Result<(), String> {
    let (signature, product) = asked_of_windows(path)?;
    microsoft_signed(&signature)?;
    visual_cpp_installer(&product)
}

/// Whether the file at `path` is signed by Microsoft under one of its
/// roots, whatever program it is: the signature check alone, which the
/// Windows test of the real check asks of a program that is not the
/// installer.
#[cfg_attr(
    not(all(test, windows)),
    expect(
        dead_code,
        reason = "asked by the Windows test of the real check alone"
    )
)]
pub(crate) fn signed_by_microsoft(path: &Path) -> Result<(), String> {
    microsoft_signed(&asked_of_windows(path)?.0)
}

/// Whether `signature` is a valid one of Microsoft's, under one of its own
/// roots, or why not.
pub(crate) fn microsoft_signed(signature: &Signature) -> Result<(), String> {
    if signature.status != "Valid" {
        return Err(format!(
            "Windows says its signature is not valid ({})",
            signature.status
        ));
    }
    let Some(signer) = &signature.signer else {
        return Err("Windows names no signer for its signature".to_owned());
    };
    if organisation(signer)?.as_deref() != Some(MICROSOFT) {
        return Err(format!("its signer is {signer}, not Microsoft"));
    }
    let Some(root) = signature.root.as_ref().filter(|_| signature.chain_built) else {
        return Err("its signer's chain did not build to a trusted root".to_owned());
    };
    if root.subject != root.issuer {
        return Err(format!(
            "its signer's chain ends at {}, which is not a root",
            root.subject
        ));
    }
    if !MICROSOFT_ROOTS
        .iter()
        .any(|(thumbprint, _)| root.thumbprint.eq_ignore_ascii_case(thumbprint))
    {
        return Err(format!(
            "it is signed under {} ({}), a root gglib does not know as one of Microsoft's own",
            root.subject, root.thumbprint
        ));
    }
    Ok(())
}

/// Whether `product` is the x64 Visual C++ runtime's installer at
/// [`FLOOR`] or later, or why not.
pub(crate) fn visual_cpp_installer(product: &Product) -> Result<(), String> {
    let name = product.name.as_deref().unwrap_or("");
    // `<name>` alone, or `<name> - <version>`, exactly.
    let named = PRODUCT_NAMES.iter().any(|known| {
        name == *known
            || name
                .strip_prefix(known)
                .and_then(|rest| rest.strip_prefix(" - "))
                .is_some_and(|version| {
                    !version.is_empty() && version.chars().all(|c| c.is_ascii_digit() || c == '.')
                })
    });
    if !named {
        return Err(format!(
            "it says it is \"{name}\", not the Microsoft Visual C++ Redistributable (x64)"
        ));
    }
    if product.version < FLOOR {
        let (major, minor) = product.version;
        return Err(format!(
            "it is version {major}.{minor}, older than {}.{}",
            FLOOR.0, FLOOR.1
        ));
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
            if attribute != "O" {
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

/// The relative names of `name`, each its attributes and their values, read
/// exactly as `CertNameToStr` writes them: `, ` between relative names,
/// ` + ` between the values of one, a value with any of `,+="` (and others)
/// in double quotes with `""` for a quote. A backslash outside quotes is
/// not in that form, and refuses the name; nothing is trimmed but the one
/// space each separator carries.
pub(crate) fn relative_names(name: &str) -> Result<Vec<Relative>, String> {
    let malformed = || format!("{name} is not a name Windows writes");
    let mut chars = name.chars().peekable();
    let mut names = Vec::new();
    let mut current: Relative = Vec::new();
    loop {
        let mut attribute = String::new();
        loop {
            match chars.next() {
                Some('=') if !attribute.is_empty() => break,
                Some(c) if c.is_ascii_alphanumeric() || c == '.' => attribute.push(c),
                _ => return Err(malformed()),
            }
        }
        let mut value = String::new();
        let terminator = if chars.peek() == Some(&'"') {
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
            // A quoted value ends at its quote: ` + `, `, ` or the end.
            match (chars.next(), chars.next()) {
                (None, _) => None,
                (Some(','), Some(' ')) => Some(','),
                (Some(' '), Some('+')) if chars.next() == Some(' ') => Some('+'),
                _ => return Err(malformed()),
            }
        } else {
            loop {
                match chars.next() {
                    None => break None,
                    Some(',') if chars.next() == Some(' ') => break Some(','),
                    // Windows quotes a value holding any of these, and
                    // never escapes with a backslash.
                    Some('\\' | '"' | ',' | '=') => return Err(malformed()),
                    Some('+') => {
                        if value.ends_with(' ') && chars.next() == Some(' ') {
                            value.pop();
                            break Some('+');
                        }
                        return Err(malformed());
                    }
                    Some(c) => value.push(c),
                }
            }
        };
        current.push((attribute, value));
        match terminator {
            Some('+') => {}
            Some(_) => names.push(std::mem::take(&mut current)),
            None => {
                names.push(current);
                return Ok(names);
            }
        }
    }
}

/// Ask Windows what it says of `path`'s signature and version information.
fn asked_of_windows(path: &Path) -> Result<(Signature, Product), String> {
    let Some(root) = std::env::var_os("SystemRoot") else {
        return Err(
            "Windows' folder is not known, so its signature could not be checked".to_owned(),
        );
    };
    let home = Path::new(&root)
        .join("System32")
        .join("WindowsPowerShell")
        .join("v1.0");
    let run = run_bounded(
        cmd(home.join("powershell.exe"))
            .args(["-NoProfile", "-NonInteractive", "-Command", SCRIPT])
            .env(PATH_VARIABLE, path)
            .env("PSModulePath", home.join("Modules")),
        VERIFY_WAIT,
    )
    .map_err(|e| format!("Windows could not check its signature ({e})"))?;
    let printed = String::from_utf8_lossy(&run.stdout);
    if printed.lines().any(|line| line.trim() == RESTRICTED) {
        return Err(
            "PowerShell on this PC is restricted (Constrained Language Mode), so gglib could \
             not check the download"
                .to_owned(),
        );
    }
    if !run.status.success() {
        return Err(format!(
            "Windows could not check its signature ({})",
            String::from_utf8_lossy(&run.stderr).trim()
        ));
    }
    parse_answer(&printed)
}

/// [`asked_of_windows`]'s answer from what the script printed: the values
/// between its markers, whatever was printed around them.
pub(crate) fn parse_answer(printed: &str) -> Result<(Signature, Product), String> {
    let unreadable =
        |why: &str| format!("Windows' answer about its signature could not be read ({why})");
    let mut lines = printed.lines().map(str::trim);
    lines
        .by_ref()
        .find(|line| *line == BEGIN)
        .ok_or_else(|| unreadable("no answer"))?;
    let mut values = std::collections::HashMap::new();
    for line in lines.by_ref() {
        if line == END {
            break;
        }
        let (name, encoded) = line.split_once('=').ok_or_else(|| unreadable(line))?;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .map_err(|e| unreadable(&e.to_string()))?;
        let text = String::from_utf8(bytes).map_err(|e| unreadable(&e.to_string()))?;
        values.insert(name.to_owned(), text);
    }
    let text = |name: &str| values.get(name).filter(|value| !value.is_empty()).cloned();
    let number = |name: &str| text(name).and_then(|value| value.parse().ok()).unwrap_or(0);
    let root = match (text("root"), text("rootIssuer"), text("rootThumbprint")) {
        (Some(subject), Some(issuer), Some(thumbprint)) => Some(Root {
            subject,
            issuer,
            thumbprint,
        }),
        _ => None,
    };
    let signature = Signature {
        status: text("status").ok_or_else(|| unreadable("no status"))?,
        signer: text("signer"),
        chain_built: text("built").as_deref() == Some("True"),
        root,
    };
    let product = Product {
        name: text("product"),
        version: (number("major"), number("minor")),
    };
    Ok((signature, product))
}

#[cfg(test)]
#[path = "engine_vc_signature_tests.rs"]
mod tests;
