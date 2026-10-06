//! Where the settings table is made, read off this crate's source.
//!
//! Making it a second time is `CREATE TABLE IF NOT EXISTS` on a table that is
//! already there, which does nothing: no test that opens a database can tell
//! one creation from two. So the rule is checked in the source. Every file
//! under `src` is read but the tests' own (`*_tests.rs`, which make tables
//! for themselves), with its comment lines dropped. `ensure_table` is private
//! to the crate, so no call to it can sit anywhere else.
//!
//! What is found is the statement written out, `CREATE TABLE` and the name
//! in any case and spacing, with or without `IF NOT EXISTS`. One put
//! together from pieces is not.

use std::fs;
use std::path::Path;

/// Each source file that is not a test's, by its path under `src`: its code
/// in lower case, every run of whitespace one space.
fn sources() -> Vec<(String, String)> {
    fn walk(dir: &Path, src: &Path, found: &mut Vec<(String, String)>) {
        for entry in fs::read_dir(dir).expect("a source directory") {
            let path = entry.expect("a directory entry").path();
            let name = path
                .strip_prefix(src)
                .expect("a path under src")
                .components()
                .map(|part| part.as_os_str().to_string_lossy())
                .collect::<Vec<_>>()
                .join("/");
            if path.is_dir() {
                walk(&path, src, found);
            } else if path.extension().is_some_and(|ext| ext == "rs")
                && !name.ends_with("_tests.rs")
            {
                let code = fs::read_to_string(&path)
                    .expect("a source file")
                    .lines()
                    .filter(|line| !line.trim_start().starts_with("//"))
                    .flat_map(str::split_whitespace)
                    .collect::<Vec<_>>()
                    .join(" ")
                    .to_lowercase();
                found.push((name, code));
            }
        }
    }
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut found = Vec::new();
    walk(&src, &src, &mut found);
    found.sort();
    found
}

/// The files `count` finds something in, and how many times in each.
fn places(count: impl Fn(&str) -> usize) -> Vec<(String, usize)> {
    sources()
        .into_iter()
        .map(|(file, code)| (file, count(&code)))
        .filter(|(_, times)| *times > 0)
        .collect()
}

#[test]
fn the_settings_table_is_defined_in_one_place_and_made_by_one_call_as_a_database_opens() {
    let definitions = places(|code| {
        code.match_indices("settings_kv")
            .filter(|(at, _)| {
                let before = code[..*at].trim_end();
                before.ends_with("create table") || before.ends_with("create table if not exists")
            })
            .count()
    });
    let calls = places(|code| {
        code.matches("ensure_table(").count() - code.matches("fn ensure_table(").count()
    });

    assert_eq!(
        definitions,
        [("repositories/sqlite_settings_repository.rs".to_owned(), 1)],
        "the statements that create settings_kv"
    );
    assert_eq!(
        calls,
        [("setup.rs".to_owned(), 1)],
        "the calls that run that statement"
    );
}
