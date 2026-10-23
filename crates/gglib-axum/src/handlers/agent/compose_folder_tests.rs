//! Tests for [`super::with_folder`]: a run's folder turns gglib's own file
//! tools on in a list that names tools, and in nothing else.

use std::collections::HashSet;

use super::{DRAW_TOOL, file_tools, with_drawing, with_folder};

fn set(names: &[&str]) -> HashSet<String> {
    names.iter().map(|name| (*name).to_owned()).collect()
}

/// A list that names a tool gains the three file tools, qualified, and only
/// when the run has a folder.
#[test]
fn a_list_gains_the_file_tools_with_a_folder() {
    let named = Some(set(&["builtin:get_current_time"]));

    let mut expected = set(&["builtin:get_current_time"]);
    expected.extend(file_tools());
    assert_eq!(with_folder(named.clone(), true), Some(expected));
    assert_eq!(with_folder(named.clone(), false), named);
}

/// `[]` is a model that calls no tool, and a folder does not change that;
/// no filter is every tool, the file tools among them once there is a
/// folder to read.
#[test]
fn no_tool_and_every_tool_stay_as_they_are() {
    for has_folder in [true, false] {
        assert_eq!(
            with_folder(Some(HashSet::new()), has_folder),
            Some(HashSet::new())
        );
        assert_eq!(with_folder(None, has_folder), None);
    }
}

/// The names are the builtins' alone, one per file tool: none matches an
/// MCP server's tool of the same bare name.
#[test]
fn the_file_tools_are_named_qualified() {
    let names: Vec<String> = file_tools().collect();
    assert_eq!(
        names,
        [
            "builtin:read_file",
            "builtin:list_directory",
            "builtin:grep_search"
        ]
    );
}

/// Counted before Draw, as `prepare_over` counts them: `[]` sent with Draw
/// is the image tool alone, folder or not.
#[test]
fn a_list_that_only_draw_names_gains_no_file_tool() {
    let filter = with_drawing(with_folder(Some(HashSet::new()), true), true);
    assert_eq!(filter, Some(set(&[DRAW_TOOL])));
}
