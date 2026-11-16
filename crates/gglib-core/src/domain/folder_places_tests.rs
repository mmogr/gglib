//! Which folders a launched chat adopts, as a pure function of a folder, an
//! OS and that OS's environment, every OS's cases run on every OS.

use std::path::{Path, PathBuf};

use super::super::{NotAdopted, adopted};
use super::*;

/// A Windows machine whose environment names its folders as Windows 11
/// does: `%SystemRoot%` in capitals, as the folder on disk is not.
fn windows() -> Places {
    Places {
        os: HostOs::Windows,
        home: Some(PathBuf::from(r"C:\Users\me")),
        windows_dir: Some(PathBuf::from(r"C:\WINDOWS")),
        program_files: vec![
            PathBuf::from(r"C:\Program Files"),
            PathBuf::from(r"C:\Program Files (x86)"),
        ],
    }
}

fn macos() -> Places {
    Places {
        os: HostOs::MacOs,
        home: Some(PathBuf::from("/Users/me")),
        windows_dir: None,
        program_files: Vec::new(),
    }
}

fn linux() -> Places {
    Places {
        os: HostOs::Linux,
        home: Some(PathBuf::from("/home/me")),
        windows_dir: None,
        program_files: Vec::new(),
    }
}

fn adopts(places: &Places, dir: &str) -> bool {
    adopted(PathBuf::from(dir), places).is_ok()
}

#[test]
fn windows_system_folders_are_not_adopted() {
    let places = windows();
    for dir in [
        r"C:\Windows\System32",
        r"C:\Windows",
        r"C:\WINDOWS\system32",
        r"\\?\C:\Windows\System32\drivers",
        r"C:\",
        r"C:\Program Files",
        r"C:\Program Files\gglib",
        r"C:\Program Files (x86)\Steam",
        r"c:\program files\WindowsApps",
    ] {
        assert!(!adopts(&places, dir), "{dir} was adopted");
    }
}

#[test]
fn windows_folders_people_keep_projects_in_are_adopted() {
    let places = windows();
    for dir in [
        r"C:\WindowsApps-project",
        r"C:\Windows-src",
        r"C:\Program Files-backup",
        r"C:\Users\me\src\gglib",
        r"C:\src\gglib",
        r"D:\",
        r"D:\projects\api",
        r"Z:\",
        r"\\nas\share\",
    ] {
        assert!(adopts(&places, dir), "{dir} was not adopted");
    }
}

/// The Windows directory is wherever `%SystemRoot%` says, not `C:\Windows`.
#[test]
fn the_windows_directory_is_the_one_the_environment_names() {
    let places = Places {
        windows_dir: Some(PathBuf::from(r"E:\WinNT")),
        program_files: Vec::new(),
        ..windows()
    };

    assert!(!adopts(&places, r"E:\WinNT\System32"));
    assert!(!adopts(&places, r"E:\"));
    assert!(adopts(&places, r"C:\Windows\System32"));
    assert!(adopts(&places, r"C:\"));
}

#[test]
fn unix_system_folders_are_not_adopted() {
    for places in [macos(), linux()] {
        for dir in [
            "/",
            "/usr",
            "/usr/bin",
            "/usr/local",
            "/bin",
            "/etc",
            "/var",
        ] {
            assert!(
                !adopts(&places, dir),
                "{dir} was adopted on {:?}",
                places.os
            );
        }
    }
    for dir in [
        "/System",
        "/System/Library/CoreServices",
        "/Library",
        "/Applications",
        "/private/etc",
        "/Users",
        "/Volumes",
    ] {
        assert!(!adopts(&macos(), dir), "{dir} was adopted on macOS");
    }
    for dir in [
        "/proc",
        "/proc/1/task",
        "/sys/class",
        "/dev",
        "/home",
        "/mnt",
    ] {
        assert!(!adopts(&linux(), dir), "{dir} was adopted on Linux");
    }
}

#[test]
fn unix_folders_people_keep_projects_in_are_adopted() {
    for places in [macos(), linux()] {
        for dir in [
            "/usr/local/src/myproject",
            "/usr/src/linux",
            "/opt/app",
            "/srv/site",
            "/tmp",
            "/tmp/scratch",
            "/usrlocal",
        ] {
            assert!(adopts(&places, dir), "{dir} not adopted on {:?}", places.os);
        }
    }
    for dir in [
        "/Users/me/src/gglib",
        "/Volumes/Work/api",
        "/Applications-dev",
        "/private/var/folders/x/T/tmp1",
    ] {
        assert!(adopts(&macos(), dir), "{dir} was not adopted on macOS");
    }
    for dir in [
        "/home/me/src/gglib",
        "/mnt/data/api",
        "/media/me/usb",
        "/run/media/me/usb/api",
        "/dev/shm/build",
    ] {
        assert!(adopts(&linux(), dir), "{dir} was not adopted on Linux");
    }
}

/// Home is refused itself, as before; a folder in it is adopted, and so is
/// another user's home, which is not this user's home folder.
#[test]
fn home_itself_is_not_adopted_and_a_folder_in_it_is() {
    assert_eq!(
        adopted(PathBuf::from("/home/me"), &linux()),
        Err(NotAdopted::Home)
    );
    assert_eq!(
        adopted(PathBuf::from(r"C:\users\ME"), &windows()),
        Err(NotAdopted::Home)
    );
    let elsewhere = Places {
        home: Some(PathBuf::from("/home/other")),
        ..linux()
    };
    assert!(adopts(&elsewhere, "/home/me"));
    assert!(adopts(&linux(), "/home/me/project"));
    assert!(adopts(&windows(), r"C:\Users\me\project"));
}

/// A system folder is named as it was given, so the line can say it.
#[test]
fn a_system_folder_is_named() {
    assert_eq!(
        adopted(PathBuf::from(r"C:\Windows\System32"), &windows()),
        Err(NotAdopted::System(PathBuf::from(r"C:\Windows\System32")))
    );
}

/// Linux compares names exactly: `/USR` is not `/usr` there.
#[test]
fn linux_compares_names_with_their_case() {
    assert!(adopts(&linux(), "/USR"));
    assert!(!adopts(&macos(), "/USR"));
}

/// A machine with no home and no Windows folders named still refuses its
/// OS's own list, and on Windows refuses nothing it was not told of.
#[test]
fn a_machine_with_nothing_named_refuses_only_its_list() {
    let bare = |os| Places {
        os,
        home: None,
        windows_dir: None,
        program_files: Vec::new(),
    };
    assert!(!adopts(&bare(HostOs::Linux), "/"));
    assert!(adopts(&bare(HostOs::Windows), r"C:\Windows"));
    assert!(adopts(&bare(HostOs::Linux), "/home/me"));
}

/// `resolved` makes each place canonical and keeps one that cannot be.
#[test]
fn resolved_keeps_a_place_that_cannot_be_resolved() {
    let gone = Path::new("/no/such/place/here").to_path_buf();
    let places = Places {
        home: Some(gone.clone()),
        ..linux()
    }
    .resolved();
    assert_eq!(places.home, Some(gone));
}
