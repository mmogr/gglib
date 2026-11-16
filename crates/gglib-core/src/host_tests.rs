use super::*;

#[test]
fn this_machine_writes_for_its_own_shell() {
    assert_eq!(HostOs::Windows.shell(), Shell::PowerShell);
    for os in [HostOs::MacOs, HostOs::Linux] {
        assert_eq!(os.shell(), Shell::Posix);
    }
}

/// An OS is read from its `std::env::consts::OS` spelling, as a value: any
/// spelling gglib does not name reads as Linux.
#[test]
fn an_os_is_read_from_its_spelling() {
    assert_eq!(HostOs::from_consts("macos"), HostOs::MacOs);
    assert_eq!(HostOs::from_consts("windows"), HostOs::Windows);
    for os in ["linux", "freebsd", "android", ""] {
        assert_eq!(HostOs::from_consts(os), HostOs::Linux, "{os}");
    }
}

#[test]
fn a_shell_and_an_os_are_their_names_on_the_wire() {
    assert_eq!(
        HostOs::ALL.map(|os| serde_json::to_value(os).unwrap()),
        ["macos", "windows", "linux"]
    );
    assert_eq!(serde_json::to_value(Shell::Posix).unwrap(), "posix");
    assert_eq!(
        serde_json::to_value(Shell::PowerShell).unwrap(),
        "powershell"
    );
}

#[test]
fn a_mac_a_pc_and_any_other_computer() {
    assert_eq!(HostOs::ALL.map(HostOs::machine), ["Mac", "PC", "computer"]);
}

/// The separators said for this machine's OS are the ones the standard
/// library splits this machine's paths on.
#[test]
fn the_separators_are_the_ones_this_machines_paths_use() {
    let std_splits: String = ['\\', '/']
        .into_iter()
        .filter(|c| std::path::is_separator(*c))
        .collect();
    assert_eq!(HostOs::current().separators(), std_splits);
}

/// `contracts/host/machines.json` is every OS's host as `GET /api/version`
/// serves it; the GUI's helper for the machine's name is held to it
/// (`tests/ts/contracts/hostWords.test.ts`). Run with
/// `GGLIB_RECORD_CONTRACTS=1` to rewrite it after a deliberate change.
#[test]
fn the_checked_in_hosts_are_every_os() {
    let mut want = serde_json::to_string_pretty(&HostOs::ALL.map(Host::of)).expect("serialise");
    want.push('\n');
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../contracts/host/machines.json");
    if std::env::var_os("GGLIB_RECORD_CONTRACTS").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).expect("make contracts/host");
        std::fs::write(&path, &want).expect("write machines.json");
    }
    let have = std::fs::read_to_string(&path).expect("read contracts/host/machines.json");
    assert!(
        have == want,
        "contracts/host/machines.json is stale; rerun with GGLIB_RECORD_CONTRACTS=1\n{want}"
    );
}
