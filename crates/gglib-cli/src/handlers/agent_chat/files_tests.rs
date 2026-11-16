//! Which `--tools` lists give a session a folder, which folders a session
//! adopts, and the line it says.

use std::io;
use std::path::{Path, PathBuf};

use gglib_core::domain::folder::{NotAdopted, Places};
use gglib_core::host::HostOs;

use super::{Chat, admits_a_file_tool, files_line, in_dir, of_session};

fn names(names: &[&str]) -> Vec<String> {
    names.iter().map(|name| (*name).to_owned()).collect()
}

fn canonical(path: &Path) -> PathBuf {
    gglib_core::paths::canonical(path).expect("a path that resolves")
}

/// Every tool admits the file tools, and so does a list naming one, bare or
/// qualified; `--no-tools`'s sentinel and a list of other tools do not.
#[test]
fn a_list_admits_a_file_tool_only_when_it_is_empty_or_names_one() {
    assert!(admits_a_file_tool(&[]));
    assert!(!admits_a_file_tool(&names(&["__none__"])));
    assert!(!admits_a_file_tool(&names(&["tavily_search"])));
    assert!(admits_a_file_tool(&names(&["read_file"])));
    assert!(admits_a_file_tool(&names(&["builtin:read_file"])));
    assert!(admits_a_file_tool(&names(&[
        "tavily_search",
        "list_directory"
    ])));
    assert!(admits_a_file_tool(&names(&["builtin:grep_search"])));
    assert!(!admits_a_file_tool(&names(&["3:read_file"])));
}

/// A folder here is named read-only.
#[test]
fn a_folder_on_this_machine_is_named_read_only() {
    let line = files_line(&Ok("/work/gglib".into()), None);
    assert_eq!(line, "Files: /work/gglib (read-only)");
}

/// Under `--remote` the line says the folder is this machine's and where
/// what is read goes.
#[test]
fn a_folder_under_remote_says_what_is_read_goes_to_the_paired_machine() {
    let line = files_line(&Ok("/work/gglib".into()), Some("desk"));
    assert_eq!(
        line,
        "Files: /work/gglib on this machine (read-only; what is read is sent to desk)"
    );
}

/// Launched in home, or in a system folder, which it names, the session
/// says the file tools are off and how to turn them on, here or under
/// `--remote`; launched in a directory it cannot read, it says that
/// instead, and has no folder.
#[test]
fn no_folder_says_why() {
    let home = "Files: off in your home folder; cd into a project to read it, or --no-tools";
    assert_eq!(files_line(&Err(NotAdopted::Home), None), home);
    assert_eq!(files_line(&Err(NotAdopted::Home), Some("desk")), home);
    let system = NotAdopted::System(r"C:\Windows\System32".into());
    assert_eq!(
        files_line(&Err(system), None),
        r"Files: off in C:\Windows\System32, a system folder; cd into a project to read it, or --no-tools"
    );

    let gone = io::Error::from(io::ErrorKind::NotFound);
    let (root, line) = in_dir(Err(gone), &Places::current(), None);
    assert_eq!(root, None);
    assert!(
        line.starts_with("Files: off; the folder this ran in cannot be read"),
        "{line}"
    );
}

/// A chat started in a system folder has no folder and says so, naming it;
/// one started in a project folder on the same machine has that folder. A
/// tempdir stands in for the Windows directory, which is a value, so this
/// runs on every OS.
#[test]
fn a_session_started_in_a_system_folder_reads_nothing_and_says_so() {
    let windows = tempfile::tempdir().expect("a folder");
    let system32 = windows.path().join("System32");
    std::fs::create_dir(&system32).expect("System32");
    let project = tempfile::tempdir().expect("a folder");
    let places = Places {
        os: HostOs::Windows,
        home: None,
        windows_dir: Some(windows.path().to_path_buf()),
        program_files: Vec::new(),
    }
    .resolved();

    let (root, line) = in_dir(Ok(system32.clone()), &places, None);
    assert_eq!(root, None);
    assert_eq!(
        line,
        format!(
            "Files: off in {}, a system folder; cd into a project to read it, or --no-tools",
            canonical(&system32).display()
        )
    );

    let (root, line) = in_dir(Ok(project.path().to_path_buf()), &places, None);
    let here = canonical(project.path());
    assert_eq!(line, format!("Files: {} (read-only)", here.display()));
    assert_eq!(root, Some(here));
}

/// A session's turns say the directory it runs in, canonical, as their
/// folder, unless its list admits no file tool or it has `--no-tools`; then
/// they say none and it says nothing, its directory unread. A test runs in
/// its crate's directory, which is neither home nor a system folder.
#[test]
fn a_session_reads_where_it_runs_unless_its_list_admits_no_file_tool() {
    let here = canonical(&std::env::current_dir().unwrap());
    let session = |tools: &[&str], no_tools| of_session(&names(tools), no_tools, None, Chat::New);

    let (folder, line) = session(&[], false);
    let here = here.display().to_string();
    assert_eq!(folder.as_deref(), Some(here.as_str()));
    assert_eq!(line, Some(format!("Files: {here} (read-only)")));
    assert_eq!(session(&["builtin:read_file"], false).0, Some(here));
    assert_eq!(session(&["tavily_search"], false), (None, None));
    assert_eq!(session(&[], true), (None, None));
}

/// A resumed chat that remembers a folder still there keeps it, one the
/// chat page chose included: its turns say none, and its line names that
/// folder. One that remembers none, or one since gone, takes the directory
/// the session runs in, as a new chat does.
#[test]
fn a_resumed_chat_keeps_a_folder_still_there_and_else_takes_the_launch_directory() {
    let chosen = tempfile::tempdir().expect("a folder");
    let chosen = chosen.path().display().to_string();
    let here = canonical(&std::env::current_dir().unwrap());
    let here = here.display().to_string();
    let launched = (
        Some(here.clone()),
        Some(format!("Files: {here} (read-only)")),
    );
    let session = |chat| of_session(&[], false, None, chat);

    let kept = session(Chat::Resumed(Some(&chosen)));
    let gone = session(Chat::Resumed(Some("/no/such/folder/here")));
    let none = session(Chat::Resumed(None));

    assert_eq!(kept, (None, Some(format!("Files: {chosen} (read-only)"))));
    assert_eq!(gone, launched);
    assert_eq!(none, launched);
    assert_eq!(session(Chat::New), launched);
}

/// A folder the chat page chose is kept on a resumed chat even where it is
/// the filesystem's root or home: only what is adopted unasked is refused.
#[test]
fn a_resumed_chat_keeps_a_chosen_folder_whatever_it_is() {
    let here = canonical(&std::env::current_dir().unwrap());
    let root = here.ancestors().last().expect("a root").to_path_buf();
    let places = Places::current();
    for chosen in [Some(root), places.home].into_iter().flatten() {
        let chosen = chosen.display().to_string();
        let (folder, line) = of_session(&[], false, None, Chat::Resumed(Some(&chosen)));
        assert_eq!(folder, None);
        assert_eq!(line, Some(format!("Files: {chosen} (read-only)")));
    }
}
