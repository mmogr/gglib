//! Whether a file is Microsoft's Visual C++ installer: its signature (the
//! status, the signer's organisation read from the name in the form Windows
//! writes it, a chain that built to one of Microsoft's two pinned roots) and
//! its product (the name and the version its version information gives),
//! each checked on its own. On Windows, the real checks of a program
//! Microsoft signed that is not the installer, and of one nobody signed, in
//! a folder whose name PowerShell would read as code were it in the script.

use super::{
    MICROSOFT_ROOTS, Product, Root, Signature, microsoft_signed, organisation, parse_answer,
    relative_names, visual_cpp_installer,
};

/// The signer's and the root's subjects as Windows writes them for
/// Microsoft's Visual C++ installer.
const SIGNER: &str = "CN=Microsoft Corporation, O=Microsoft Corporation, L=Redmond, \
                      S=Washington, C=US";
const ROOT: &str = "CN=Microsoft Root Certificate Authority 2011, O=Microsoft Corporation, \
                    L=Redmond, S=Washington, C=US";

fn root_2011() -> Root {
    Root {
        subject: ROOT.to_owned(),
        issuer: ROOT.to_owned(),
        thumbprint: MICROSOFT_ROOTS[0].0.to_owned(),
    }
}

fn signature(status: &str, signer: Option<&str>) -> Signature {
    Signature {
        status: status.to_owned(),
        signer: signer.map(str::to_owned),
        chain_built: true,
        root: Some(root_2011()),
    }
}

/// Microsoft's installer, as Windows describes it, is accepted, under
/// either pinned root.
#[test]
fn a_valid_signature_of_microsoft_s_under_its_root_is_accepted() {
    assert_eq!(microsoft_signed(&signature("Valid", Some(SIGNER))), Ok(()));
    let under_2010 = Signature {
        root: Some(Root {
            thumbprint: MICROSOFT_ROOTS[1].0.to_lowercase(),
            ..root_2011()
        }),
        ..signature("Valid", Some(SIGNER))
    };
    assert_eq!(microsoft_signed(&under_2010), Ok(()));
}

/// The roots are the two Microsoft's PKI repository lists, as their
/// certificates hash.
#[test]
fn the_pinned_roots_are_microsoft_s_two() {
    assert_eq!(
        MICROSOFT_ROOTS,
        [
            (
                "8F43288AD272F3103B6FB1428485EA3014C0BCFE",
                "Microsoft Root Certificate Authority 2011"
            ),
            (
                "3B1EFD3A66EA28B16697394703A72CA340A05BD5",
                "Microsoft Root Certificate Authority 2010"
            ),
        ]
    );
}

/// Anything not valid, or signed by another, is refused.
#[test]
fn an_invalid_or_another_s_signature_is_refused() {
    for (status, signer) in [
        ("NotSigned", None),
        ("HashMismatch", Some(SIGNER)),
        ("NotTrusted", Some(SIGNER)),
        ("Valid", None),
        ("Valid", Some("CN=Someone, O=Someone Else, C=US")),
    ] {
        assert!(
            microsoft_signed(&signature(status, signer)).is_err(),
            "{status} {signer:?}"
        );
    }
}

/// A chain that did not build, one that ends at a certificate that is not
/// self-signed (an intermediate, on a partial chain), and one that ends at a
/// root gglib does not pin (a Microsoft-named root of its signing service,
/// say) are each refused, the last saying gglib does not know the root.
#[test]
fn only_a_built_chain_to_a_pinned_root_is_accepted() {
    let not_built = Signature {
        chain_built: false,
        ..signature("Valid", Some(SIGNER))
    };
    let intermediate = Signature {
        root: Some(Root {
            subject: "CN=Microsoft Code Signing PCA 2011, O=Microsoft Corporation, C=US".to_owned(),
            issuer: ROOT.to_owned(),
            thumbprint: "F252E794FE438E35ACE6E53762C0A234A2C52135".to_owned(),
        }),
        ..signature("Valid", Some(SIGNER))
    };
    let elsewhere_subject = "CN=Microsoft Identity Verification Root Certificate Authority 2020, O=Microsoft \
         Corporation, C=US";
    let unknown_root = Signature {
        root: Some(Root {
            subject: elsewhere_subject.to_owned(),
            issuer: elsewhere_subject.to_owned(),
            thumbprint: "0000000000000000000000000000000000000000".to_owned(),
        }),
        ..signature("Valid", Some(SIGNER))
    };
    let no_root = Signature {
        root: None,
        ..signature("Valid", Some(SIGNER))
    };

    for refused in [&not_built, &intermediate, &no_root] {
        assert!(microsoft_signed(refused).is_err(), "{refused:?}");
    }
    let unknown = microsoft_signed(&unknown_root).unwrap_err();
    assert!(
        unknown.contains("a root gglib does not know as one of Microsoft's own"),
        "{unknown}"
    );
}

/// A value that only contains `O=Microsoft Corporation` is not the
/// organisation: quoted, in another attribute, or in a longer name; and a
/// name with a space other than an ASCII one after it is not the name.
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
        (
            "CN=\"He said \"\"hi\"\"\", O=Quoted Inc",
            Some("Quoted Inc"),
        ),
        ("CN=\"a\\b\", O=Back Slash Inc", Some("Back Slash Inc")),
    ] {
        assert_eq!(
            organisation(name).unwrap().as_deref(),
            organisation_named,
            "{name}"
        );
    }
    for space in ['\u{a0}', '\u{2003}', '\t'] {
        let name = format!("CN=Someone, O=Microsoft Corporation{space}, C=US");
        assert_ne!(
            organisation(&name).ok().flatten().as_deref(),
            Some("Microsoft Corporation"),
            "{name:?}"
        );
        assert!(
            microsoft_signed(&signature("Valid", Some(&name))).is_err(),
            "{name:?}"
        );
    }
    for name in [
        "CN=\"x, O=Microsoft Corporation, y\", O=Evil Ltd, C=US",
        "CN=Someone, O=Microsoft Corporation Ltd, C=US",
        "CN=Someone, OU=\"O=Microsoft Corporation\", C=US",
    ] {
        assert!(
            microsoft_signed(&signature("Valid", Some(name))).is_err(),
            "{name}"
        );
    }
}

/// A backslash outside quotes is not in the form Windows writes, and
/// refuses the whole name: read as an escape it could hide an attribute or
/// make one up.
#[test]
fn a_backslash_outside_quotes_refuses_the_name() {
    for name in [
        "CN=Evil\\, OU=\"x, O=Microsoft Corporation, Z=w\", C=US",
        "CN=x\\, O=Evil Corp, C=US",
        "CN=Someone, OU=O\\=Microsoft Corporation, C=US",
    ] {
        assert!(organisation(name).is_err(), "{name}");
        assert!(
            microsoft_signed(&signature("Valid", Some(name))).is_err(),
            "{name}"
        );
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
        assert!(
            microsoft_signed(&signature("Valid", Some(name))).is_err(),
            "{name}"
        );
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
        "=x, O=y",
        "CN=a,O=b",
        "CN=a+b, O=c",
        " CN=a, O=b",
        "",
    ] {
        assert!(organisation(name).is_err(), "{name:?}");
    }
}

fn product(name: &str, version: (u32, u32)) -> Product {
    Product {
        name: Some(name.to_owned()),
        version,
    }
}

/// The installer as Microsoft names it, now and since Visual C++ 2022, at
/// 14.30 or later, is the product.
#[test]
fn the_runtime_s_installer_is_the_product() {
    for accepted in [
        product(
            "Microsoft Visual C++ v14 Redistributable (x64) - 14.52.37029",
            (14, 52),
        ),
        product(
            "Microsoft Visual C++ 2015-2022 Redistributable (x64) - 14.44.35211",
            (14, 44),
        ),
        product(
            "Microsoft Visual C++ 2015-2022 Redistributable (x64)",
            (14, 30),
        ),
    ] {
        assert_eq!(visual_cpp_installer(&accepted), Ok(()), "{accepted:?}");
    }
}

/// Another Microsoft program, the x86 installer, an older one and a name
/// with something after it are not.
#[test]
fn another_program_or_an_older_installer_is_not_the_product() {
    for refused in [
        product("Microsoft® Windows® Operating System", (10, 0)),
        product(
            "Microsoft Visual C++ v14 Redistributable (x86) - 14.52.37029",
            (14, 52),
        ),
        product(
            "Microsoft Visual C++ 2015-2019 Redistributable (x64) - 14.29.30139",
            (14, 29),
        ),
        product(
            "Microsoft Visual C++ 2015-2022 Redistributable (x64) - 14.29.30139",
            (14, 29),
        ),
        product(
            "Microsoft Visual C++ v14 Redistributable (x64) - 14.52 and more",
            (14, 52),
        ),
        Product::default(),
    ] {
        assert!(visual_cpp_installer(&refused).is_err(), "{refused:?}");
    }
}

/// What PowerShell prints is read between its markers, whatever is printed
/// before them.
#[test]
fn the_answer_is_read_between_its_markers() {
    use base64::Engine as _;
    let encode = |text: &str| base64::engine::general_purpose::STANDARD.encode(text);
    let printed = format!(
        "WARNING: something PowerShell wanted to say\r\nGGLIB-BEGIN\r\nstatus={}\r\nsigner={}\r\n\
         built={}\r\nroot={}\r\nrootIssuer={}\r\nrootThumbprint={}\r\nproduct={}\r\nmajor={}\r\n\
         minor={}\r\nGGLIB-END\r\n",
        encode("Valid"),
        encode(SIGNER),
        encode("True"),
        encode(ROOT),
        encode(ROOT),
        encode(MICROSOFT_ROOTS[0].0),
        encode("Microsoft Visual C++ v14 Redistributable (x64) - 14.52.37029"),
        encode("14"),
        encode("52"),
    );

    let (read_signature, read_product) = parse_answer(&printed).unwrap();

    assert_eq!(read_signature, signature("Valid", Some(SIGNER)));
    assert_eq!(
        read_product,
        product(
            "Microsoft Visual C++ v14 Redistributable (x64) - 14.52.37029",
            (14, 52)
        )
    );
    let unsigned = "GGLIB-BEGIN\nstatus=Tm90U2lnbmVk\nsigner=\nbuilt=RmFsc2U=\nroot=\n\
                    rootIssuer=\nrootThumbprint=\nproduct=\nmajor=MA==\nminor=MA==\nGGLIB-END\n";
    let (read_signature, _) = parse_answer(unsigned).unwrap();
    assert_eq!(read_signature.status, "NotSigned");
    assert_eq!(read_signature.signer, None);
    assert!(parse_answer("Get-AuthenticodeSignature : not recognised").is_err());
}

/// The real checks, on Windows. A program Microsoft signed (PowerShell
/// itself) passes the signature check, also copied into a folder whose name
/// holds quotes, `$(`, a backtick and `;`, which PowerShell would read as
/// code were the path in its script; it is refused by the product check. A
/// file nobody signed there is refused by what Windows said of it, not by a
/// script that broke.
#[cfg(windows)]
#[test]
fn the_real_checks_take_microsoft_s_signature_and_refuse_another_program() {
    use super::{installer_checked, signed_by_microsoft};

    let root = std::env::var_os("SystemRoot").expect("Windows names its folder");
    let powershell = std::path::Path::new(&root)
        .join("System32")
        .join("WindowsPowerShell")
        .join("v1.0")
        .join("powershell.exe");
    assert_eq!(signed_by_microsoft(&powershell), Ok(()));
    let not_the_product = installer_checked(&powershell).unwrap_err();
    assert!(
        not_the_product.starts_with("it says it is"),
        "{not_the_product}"
    );

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
