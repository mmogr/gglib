//! Which `--tools` lists give a session a folder, and the line it says.

use std::io;
use std::path::Path;

use super::{AgentSessionParams, admits_a_file_tool, files_line, in_dir, of_session};
use crate::handlers::agent_chat::resume_settings::tests::chat_args;
use crate::handlers::inference::chat::ChatArgs;

fn names(names: &[&str]) -> Vec<String> {
    names.iter().map(|name| (*name).to_owned()).collect()
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
    let line = files_line(Some(Path::new("/work/gglib")), None);
    assert_eq!(line, "Files: /work/gglib (read-only)");
}

/// Under `--remote` the line says the folder is this machine's and where
/// what is read goes.
#[test]
fn a_folder_under_remote_says_what_is_read_goes_to_the_paired_machine() {
    let line = files_line(Some(Path::new("/work/gglib")), Some("desk"));
    assert_eq!(
        line,
        "Files: /work/gglib on this machine (read-only; what is read is sent to desk)"
    );
}

/// Launched in home or `/`, the session says the file tools are off and
/// how to turn them on, naming both, here or under `--remote`; launched in
/// a directory it cannot read, it says that instead, and has no folder.
#[test]
fn no_folder_says_why() {
    let off = "Files: off in your home folder or /; cd into a project to read it, or --no-tools";
    assert_eq!(files_line(None, None), off);
    assert_eq!(files_line(None, Some("desk")), off);
    assert_eq!(in_dir(Ok("/".into()), None), (None, Some(off.to_owned())));

    let gone = io::Error::from(io::ErrorKind::NotFound);
    let (root, line) = in_dir(Err(gone), None);
    assert_eq!(root, None);
    let line = line.expect("a line");
    assert!(
        line.starts_with("Files: off; the folder this ran in cannot be read"),
        "{line}"
    );
}

/// A session reads the directory it runs in, canonical, unless its list
/// admits no file tool; then it has no folder and says nothing, its
/// directory unread. A test runs in its crate's directory, which is
/// neither home nor `/`.
#[test]
fn a_session_reads_where_it_runs_unless_its_list_admits_no_file_tool() {
    let here = std::env::current_dir().unwrap().canonicalize().unwrap();
    let session = |tools: &[&str], no_tools| {
        let args = ChatArgs {
            tools: names(tools),
            no_tools,
            ..chat_args()
        };
        of_session(&AgentSessionParams::from(&args))
    };

    let (root, line) = session(&[], false);
    assert_eq!(root.as_deref(), Some(here.as_path()));
    assert_eq!(line, Some(format!("Files: {} (read-only)", here.display())));
    assert_eq!(session(&["builtin:read_file"], false).0, Some(here));
    assert_eq!(session(&["tavily_search"], false), (None, None));
    assert_eq!(session(&[], true), (None, None));
}
