//! Whether a signature is Microsoft's: its status, and the organisation of
//! its signer and of its root, read from names in the form Windows writes
//! them. On Windows, the real check of a file Microsoft signed and of one
//! nobody did, in a folder whose name PowerShell would read as code if it
//! were pasted into the script.

use super::{Signature, microsoft_signed, organisation, parse_signature, relative_names};

/// The signer's and the root's subjects as Windows writes them for
/// Microsoft's Visual C++ installer.
const SIGNER: &str = "CN=Microsoft Corporation, O=Microsoft Corporation, L=Redmond, \
                      S=Washington, C=US";
const ROOT: &str = "CN=Microsoft Root Certificate Authority 2011, O=Microsoft Corporation, \
                    L=Redmond, S=Washington, C=US";

fn signature(status: &str, signer: Option<&str>, root: Option<&str>) -> Signature {
    Signature {
        status: status.to_owned(),
        signer: signer.map(str::to_owned),
        root: root.map(str::to_owned),
    }
}

/// Microsoft's installer, as Windows describes it, is accepted.
#[test]
fn a_valid_signature_of_microsoft_s_under_its_root_is_run() {
    assert_eq!(
        microsoft_signed(&signature("Valid", Some(SIGNER), Some(ROOT))),
        Ok(())
    );
}

/// Anything not valid, signed by another, or under another root is not.
#[test]
fn anything_else_is_refused() {
    let elsewhere = "CN=Some Root CA, O=Some Trust Services, C=US";
    for (status, signer, root) in [
        ("NotSigned", None, None),
        ("HashMismatch", Some(SIGNER), Some(ROOT)),
        ("NotTrusted", Some(SIGNER), Some(ROOT)),
        ("Valid", None, Some(ROOT)),
        ("Valid", Some(SIGNER), None),
        ("Valid", Some(SIGNER), Some(elsewhere)),
        (
            "Valid",
            Some("CN=Someone, O=Someone Else, C=US"),
            Some(ROOT),
        ),
    ] {
        assert!(
            microsoft_signed(&signature(status, signer, root)).is_err(),
            "{status} {signer:?} {root:?}"
        );
    }
}

/// A value that only contains `O=Microsoft Corporation` is not the
/// organisation: quoted, in another attribute, or in a longer name.
#[test]
fn only_the_organisation_itself_counts() {
    for (name, organisation_named) in [
        (SIGNER, Some("Microsoft Corporation")),
        (
            "CN=\"x, O=Microsoft Corporation, y\", O=Evil Ltd, C=US",
            Some("Evil Ltd"),
        ),
        (
            "CN=Someone, O=Microsoft Corporation Ltd, C=US",
            Some("Microsoft Corporation Ltd"),
        ),
        ("CN=Someone, OU=\"O=Microsoft Corporation\", C=US", None),
        ("CN=Someone, OU=O\\=Microsoft Corporation, C=US", None),
        (
            "CN=\"He said \"\"hi\"\"\", O=Quoted Inc",
            Some("Quoted Inc"),
        ),
    ] {
        assert_eq!(
            organisation(name).unwrap().as_deref(),
            organisation_named,
            "{name}"
        );
    }
    for name in [
        "CN=\"x, O=Microsoft Corporation, y\", O=Evil Ltd, C=US",
        "CN=Someone, O=Microsoft Corporation Ltd, C=US",
        "CN=Someone, OU=\"O=Microsoft Corporation\", C=US",
    ] {
        let signed = signature("Valid", Some(name), Some(ROOT));
        assert!(microsoft_signed(&signed).is_err(), "{name}");
    }
}

/// An organisation that shares a relative name with another attribute, or
/// a name with two organisations, is not read as naming one.
#[test]
fn a_multi_valued_or_doubled_organisation_is_refused() {
    for name in [
        "CN=Evil + O=Microsoft Corporation, C=US",
        "O=Microsoft Corporation, O=Evil Ltd, C=US",
    ] {
        assert!(organisation(name).is_err(), "{name}");
        let signed = signature("Valid", Some(name), Some(ROOT));
        assert!(microsoft_signed(&signed).is_err(), "{name}");
    }
    assert_eq!(
        relative_names("CN=Evil + OU=Sales, O=Microsoft Corporation").unwrap(),
        [
            vec![
                ("CN".to_owned(), "Evil".to_owned()),
                ("OU".to_owned(), "Sales".to_owned())
            ],
            vec![("O".to_owned(), "Microsoft Corporation".to_owned())],
        ]
    );
    assert_eq!(
        organisation("CN=Evil + OU=Sales, O=Microsoft Corporation").unwrap(),
        Some("Microsoft Corporation".to_owned())
    );
}

/// A name that is not in Windows' form is not read at all.
#[test]
fn a_malformed_name_is_refused() {
    for name in [
        "CN=\"unclosed, O=Microsoft Corporation",
        "no equals sign",
        "=x, O",
    ] {
        assert!(organisation(name).is_err(), "{name}");
    }
}

/// What PowerShell prints is read as the status, the signer and the root.
#[test]
fn the_check_is_read_from_its_json() {
    let printed =
        format!("\u{feff}{{\"root\":\"{ROOT}\",\"status\":\"Valid\",\"signer\":\"{SIGNER}\"}}\r\n");
    assert_eq!(
        parse_signature(&printed).unwrap(),
        signature("Valid", Some(SIGNER), Some(ROOT))
    );
    assert_eq!(
        parse_signature("{\"root\":null,\"status\":\"NotSigned\",\"signer\":null}").unwrap(),
        signature("NotSigned", None, None)
    );
    assert!(parse_signature("Get-AuthenticodeSignature : not recognised").is_err());
}

/// The real check, on Windows: a program Microsoft signed is accepted, also
/// copied into a folder whose name holds quotes, `$(`, a backtick and `;`,
/// which PowerShell would read as code were the path in its script; a file
/// nobody signed there is refused by what Windows said of it, not by a
/// script that broke.
#[cfg(windows)]
#[test]
fn the_real_check_accepts_microsoft_and_refuses_the_unsigned() {
    use super::signed_by_microsoft;

    let root = std::env::var_os("SystemRoot").expect("Windows names its folder");
    let powershell = std::path::Path::new(&root)
        .join("System32")
        .join("WindowsPowerShell")
        .join("v1.0")
        .join("powershell.exe");
    assert_eq!(signed_by_microsoft(&powershell), Ok(()));

    let parent = tempfile::tempdir().unwrap();
    let odd = parent.path().join("O\u{2019}Brien's $(whoami) `x; y");
    std::fs::create_dir(&odd).unwrap();
    let copied = odd.join("powershell.exe");
    std::fs::copy(&powershell, &copied).unwrap();
    assert_eq!(signed_by_microsoft(&copied), Ok(()));

    let unsigned = odd.join("unsigned.exe");
    std::fs::write(&unsigned, b"nobody signed this").unwrap();
    let refused = signed_by_microsoft(&unsigned).unwrap_err();
    assert!(
        refused.starts_with("Windows says its signature is not valid"),
        "{refused}"
    );
}
