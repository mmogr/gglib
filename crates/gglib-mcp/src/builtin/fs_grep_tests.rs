//! Tests for `builtin:grep_search`.

use super::*;

/// A search nobody has stopped.
fn grep(args: &HashMap<String, Value>, root: &Path) -> Result<String, String> {
    grep_search(args, root, &AtomicBool::new(false))
}

fn args_with_pattern(pattern: &str) -> HashMap<String, Value> {
    let mut m = HashMap::new();
    m.insert("pattern".to_string(), Value::String(pattern.to_string()));
    m
}

fn args_with_pattern_and_path(pattern: &str, path: &str) -> HashMap<String, Value> {
    let mut m = args_with_pattern(pattern);
    m.insert("path".to_string(), Value::String(path.to_string()));
    m
}

#[test]
fn finds_matching_lines() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("code.rs"), "fn hello() {}\nfn world() {}\n").unwrap();

    let result = grep(&args_with_pattern("hello"), dir.path()).unwrap();
    assert!(result.contains("code.rs:1:fn hello()"));
}

#[test]
fn case_insensitive() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("code.rs"), "fn Hello() {}\n").unwrap();

    let result = grep(&args_with_pattern("hello"), dir.path()).unwrap();
    assert!(result.contains("Hello"));
}

#[test]
fn no_matches() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("code.rs"), "fn hello() {}\n").unwrap();

    let result = grep(&args_with_pattern("xyz_no_match"), dir.path()).unwrap();
    assert!(result.contains("no matches"));
}

#[test]
fn searches_subdirectories() {
    let dir = tempfile::tempdir().unwrap();
    let sub = dir.path().join("sub");
    fs::create_dir(&sub).unwrap();
    fs::write(sub.join("nested.rs"), "fn deep_func() {}\n").unwrap();

    let result = grep(&args_with_pattern("deep_func"), dir.path()).unwrap();
    assert!(result.contains("sub/nested.rs:1:fn deep_func()"));
}

#[test]
fn skips_binary_files() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("bin.dat"), b"match\x00here").unwrap();

    let result = grep(&args_with_pattern("match"), dir.path()).unwrap();
    assert!(result.contains("no matches"));
}

#[test]
fn search_single_file() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("a.rs"), "fn hello() {}\n").unwrap();
    fs::write(dir.path().join("b.rs"), "fn hello() {}\n").unwrap();

    let result = grep(&args_with_pattern_and_path("hello", "a.rs"), dir.path()).unwrap();
    assert!(result.contains("a.rs"));
    // Should not contain b.rs since we targeted a.rs only
    assert!(!result.contains("b.rs"));
}

#[test]
fn missing_pattern() {
    let dir = tempfile::tempdir().unwrap();
    let err = grep(&HashMap::new(), dir.path()).unwrap_err();
    assert!(err.contains("pattern"));
}

#[test]
fn empty_pattern() {
    let dir = tempfile::tempdir().unwrap();
    let err = grep(&args_with_pattern(""), dir.path()).unwrap_err();
    assert!(err.contains("empty"));
}

#[test]
fn skips_git_and_node_modules() {
    let dir = tempfile::tempdir().unwrap();
    let git = dir.path().join(".git");
    fs::create_dir(&git).unwrap();
    fs::write(git.join("config"), "match_me\n").unwrap();

    let nm = dir.path().join("node_modules");
    fs::create_dir(&nm).unwrap();
    fs::write(nm.join("pkg.js"), "match_me\n").unwrap();

    fs::write(dir.path().join("src.rs"), "match_me\n").unwrap();

    let result = grep(&args_with_pattern("match_me"), dir.path()).unwrap();
    // Only src.rs should match
    assert!(result.contains("src.rs"));
    assert!(!result.contains(".git"));
    assert!(!result.contains("node_modules"));
}

#[test]
fn a_stopped_walk_collects_no_matches() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("code.rs"), "match_me\n").unwrap();

    let stopped = AtomicBool::new(true);
    let result = grep_search(&args_with_pattern("match_me"), dir.path(), &stopped).unwrap();
    assert!(result.contains("no matches"));
}

#[test]
fn paths_are_relative_to_a_root_given_by_another_name() {
    let dir = tempfile::tempdir().unwrap();
    let sub = dir.path().join("sub");
    fs::create_dir(&sub).unwrap();
    fs::write(sub.join("code.rs"), "match_me\n").unwrap();
    let roundabout = sub.join("..").join("sub");

    let result = grep(&args_with_pattern("match_me"), &roundabout).unwrap();
    assert_eq!(result, "code.rs:1:match_me");
}

#[cfg(unix)]
#[test]
fn a_symlink_out_of_the_root_is_not_searched() {
    let outside = tempfile::tempdir().unwrap();
    fs::write(outside.path().join("secret.txt"), "match_me\n").unwrap();
    let dir = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink(outside.path(), dir.path().join("away")).unwrap();
    std::os::unix::fs::symlink(
        outside.path().join("secret.txt"),
        dir.path().join("secret_link.txt"),
    )
    .unwrap();

    let result = grep(&args_with_pattern("match_me"), dir.path()).unwrap();
    assert!(result.contains("no matches"), "{result}");
}

#[cfg(unix)]
#[test]
fn a_symlink_inside_the_root_is_searched() {
    let dir = tempfile::tempdir().unwrap();
    let real = dir.path().join("real");
    fs::create_dir(&real).unwrap();
    fs::write(real.join("code.rs"), "match_me\n").unwrap();
    std::os::unix::fs::symlink(&real, dir.path().join("alias")).unwrap();
    std::os::unix::fs::symlink(real.join("code.rs"), dir.path().join("linked.rs")).unwrap();

    let result = grep(&args_with_pattern("match_me"), dir.path()).unwrap();
    assert!(result.contains("alias/code.rs:1:match_me"), "{result}");
    assert!(result.contains("linked.rs:1:match_me"), "{result}");
    assert!(result.contains("real/code.rs:1:match_me"), "{result}");
}

#[cfg(unix)]
#[test]
fn a_link_to_its_own_folder_or_above_lists_each_match_once() {
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("a");
    fs::create_dir(&a).unwrap();
    fs::write(a.join("x.rs"), "match_me\n").unwrap();
    std::os::unix::fs::symlink(".", a.join("l1")).unwrap();
    std::os::unix::fs::symlink("..", a.join("up")).unwrap();

    let result = grep(&args_with_pattern("match_me"), dir.path()).unwrap();
    assert_eq!(result, "a/x.rs:1:match_me");
}

#[cfg(unix)]
#[test]
fn two_links_to_the_root_list_each_match_once() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("code.rs"), "match_me\n").unwrap();
    std::os::unix::fs::symlink(dir.path(), dir.path().join("a")).unwrap();
    std::os::unix::fs::symlink(dir.path(), dir.path().join("b")).unwrap();

    let result = grep(&args_with_pattern("match_me"), dir.path()).unwrap();
    assert_eq!(result, "code.rs:1:match_me");
    let none = grep(&args_with_pattern("absent"), dir.path()).unwrap();
    assert!(none.contains("no matches"), "{none}");
}

#[cfg(unix)]
#[test]
fn a_walk_below_the_root_does_not_come_back_through_a_link_above_it() {
    let dir = tempfile::tempdir().unwrap();
    let sub = dir.path().join("sub");
    fs::create_dir(&sub).unwrap();
    fs::write(sub.join("x.rs"), "match_me\n").unwrap();
    std::os::unix::fs::symlink("..", sub.join("up")).unwrap();

    let result = grep(&args_with_pattern_and_path("match_me", "sub"), dir.path()).unwrap();
    assert_eq!(result, "sub/x.rs:1:match_me");
}

#[cfg(unix)]
#[test]
fn a_dangling_link_is_passed_over() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("code.rs"), "match_me\n").unwrap();
    std::os::unix::fs::symlink(dir.path().join("gone"), dir.path().join("broken")).unwrap();

    let result = grep(&args_with_pattern("match_me"), dir.path()).unwrap();
    assert_eq!(result, "code.rs:1:match_me");
}

/// The macOS shape: the root is reached through a link (`/tmp` is
/// `/private/tmp` there).
#[cfg(unix)]
#[test]
fn matches_under_a_root_behind_a_link_are_relative_to_it() {
    let tmp = tempfile::tempdir().unwrap();
    let real = tmp.path().join("real");
    fs::create_dir_all(real.join("src")).unwrap();
    fs::write(real.join("src").join("main.rs"), "match_me\n").unwrap();
    let link = tmp.path().join("link");
    std::os::unix::fs::symlink(&real, &link).unwrap();

    let result = grep(&args_with_pattern("match_me"), &link).unwrap();
    assert_eq!(result, "src/main.rs:1:match_me");
    let through = link.join("src");
    let result = grep(
        &args_with_pattern_and_path("match_me", through.to_str().unwrap()),
        &link,
    )
    .unwrap();
    assert_eq!(result, "src/main.rs:1:match_me");
}
