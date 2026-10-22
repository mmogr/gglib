//! Tests for sandbox path resolution and its refusals.

use super::*;
use std::fs;

#[test]
fn relative_path_stays_inside() {
    let dir = tempfile::tempdir().unwrap();
    let child = dir.path().join("hello.txt");
    fs::write(&child, "hi").unwrap();

    let resolved = resolve_sandboxed_path(dir.path(), "hello.txt")
        .unwrap()
        .path;
    assert_eq!(resolved, child.canonicalize().unwrap());
}

#[test]
fn absolute_path_is_rebased() {
    let dir = tempfile::tempdir().unwrap();
    let child = dir.path().join("hello.txt");
    fs::write(&child, "hi").unwrap();

    let resolved = resolve_sandboxed_path(dir.path(), "/hello.txt")
        .unwrap()
        .path;
    assert_eq!(resolved, child.canonicalize().unwrap());
}

#[test]
fn absolute_path_inside_root_is_kept() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir(dir.path().join("src")).unwrap();
    let child = dir.path().join("src").join("main.rs");
    fs::write(&child, "fn main() {}").unwrap();

    let under = Path::new("src").join("main.rs").display().to_string();

    let given = resolve_sandboxed_path(dir.path(), child.to_str().unwrap()).unwrap();
    assert_eq!(given.path, child.canonicalize().unwrap());
    assert_eq!(given.shown, under);

    let canonical = child.canonicalize().unwrap();
    let resolved = resolve_sandboxed_path(dir.path(), canonical.to_str().unwrap()).unwrap();
    assert_eq!(resolved.path, canonical);
    assert_eq!(resolved.shown, under);
}

#[test]
fn absolute_root_itself_is_kept() {
    let dir = tempfile::tempdir().unwrap();
    let resolved = resolve_sandboxed_path(dir.path(), dir.path().to_str().unwrap()).unwrap();
    assert_eq!(resolved.path, dir.path().canonicalize().unwrap());
    assert_eq!(resolved.shown, ".");
}

#[test]
fn a_miss_names_the_path_under_the_root_and_the_next_step() {
    let dir = tempfile::tempdir().unwrap();

    let relative = resolve_sandboxed_path(dir.path(), "src/mian.rs").unwrap_err();
    assert_eq!(
        relative,
        "No file or folder 'src/mian.rs' in the working folder. Call list_directory to see \
         what is there."
    );

    let under = Path::new("src").join("mian.rs");
    let absolute = dir.path().join(&under);
    let err = resolve_sandboxed_path(dir.path(), absolute.to_str().unwrap()).unwrap_err();
    assert_eq!(
        err,
        format!("No file or folder '{}' {MISSING}", under.display())
    );
}

#[test]
fn a_rebased_miss_names_the_path_as_written() {
    let dir = tempfile::tempdir().unwrap();
    let err = resolve_sandboxed_path(dir.path(), "/etc/passwd").unwrap_err();
    assert_eq!(err, format!("No file or folder '/etc/passwd' {MISSING}"));
}

#[test]
fn an_absolute_path_that_climbs_out_is_outside() {
    let dir = tempfile::tempdir().unwrap();
    let child = dir.path().join("sub");
    fs::create_dir(&child).unwrap();
    let climb = child.join("..");

    let err = resolve_sandboxed_path(&child, climb.to_str().unwrap()).unwrap_err();
    assert_eq!(err, format!("'..' {OUTSIDE}."));
}

#[test]
fn refusals_never_say_sandbox_or_the_root() {
    let dir = tempfile::tempdir().unwrap();
    let child = dir.path().join("sub");
    fs::create_dir(&child).unwrap();
    let canon = child.canonicalize().unwrap();
    let root = canon.display().to_string();
    let inside = canon.join("nope.txt");

    for path in ["../", "nope.txt", "/nope.txt", inside.to_str().unwrap()] {
        let err = resolve_sandboxed_path(&child, path).unwrap_err();
        assert!(!err.contains("sandbox"), "{err}");
        assert!(!err.contains(&root), "{err}");
    }
    let gone = dir.path().join("gone");
    let err = resolve_sandboxed_path(&gone, "x").unwrap_err();
    assert!(!err.contains("sandbox"), "{err}");
}

#[test]
fn a_path_through_a_file_is_a_miss() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("notes.txt"), "hi").unwrap();

    let err = resolve_sandboxed_path(dir.path(), "notes.txt/inner").unwrap_err();
    assert_eq!(
        err,
        format!("No file or folder 'notes.txt/inner' {MISSING}")
    );
}

#[cfg(unix)]
#[test]
fn an_unreadable_path_says_why_and_not_that_it_is_missing() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let locked = dir.path().join("locked");
    fs::create_dir(&locked).unwrap();
    fs::write(locked.join("inside.txt"), "hi").unwrap();
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();

    let result = resolve_sandboxed_path(dir.path(), "locked/inside.txt");
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();

    // Root reads past the permission bits; there is nothing to check then.
    if let Err(err) = result {
        assert!(
            err.starts_with("'locked/inside.txt' cannot be read: "),
            "{err}"
        );
        assert!(!err.contains(MISSING), "{err}");
        assert!(!err.contains("sandbox"), "{err}");
    }
}

/// The macOS shape: the root is reached through a link (`/tmp` is
/// `/private/tmp` there).
#[cfg(unix)]
#[test]
fn a_root_behind_a_link_takes_either_form() {
    let tmp = tempfile::tempdir().unwrap();
    let real = tmp.path().join("real");
    fs::create_dir_all(real.join("src")).unwrap();
    fs::write(real.join("src").join("main.rs"), "match_me\n").unwrap();
    let link = tmp.path().join("link");
    std::os::unix::fs::symlink(&real, &link).unwrap();
    let canon_file = real.canonicalize().unwrap().join("src").join("main.rs");

    for root in [&link, &real] {
        for base in [&link, &real] {
            let given = base.join("src").join("main.rs");
            let resolved = resolve_sandboxed_path(root, given.to_str().unwrap()).unwrap();
            assert_eq!(resolved.path, canon_file);
            assert_eq!(resolved.shown, "src/main.rs");

            let miss = base.join("src").join("mian.rs");
            let err = resolve_sandboxed_path(root, miss.to_str().unwrap()).unwrap_err();
            assert_eq!(err, format!("No file or folder 'src/mian.rs' {MISSING}"));
        }
    }
}

#[test]
fn dotdot_escape_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let child = dir.path().join("sub");
    fs::create_dir(&child).unwrap();

    let result = resolve_sandboxed_path(&child, "../");
    assert_eq!(result.unwrap_err(), format!("'../' {OUTSIDE}."));
}

#[test]
fn nonexistent_path_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let result = resolve_sandboxed_path(dir.path(), "no_such_file.txt");
    assert_eq!(
        result.unwrap_err(),
        format!("No file or folder 'no_such_file.txt' {MISSING}")
    );
}

#[test]
fn empty_path_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let result = resolve_sandboxed_path(dir.path(), "");
    assert!(result.is_err());
}

#[test]
fn is_binary_detects_null_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let bin = dir.path().join("binary.dat");
    fs::write(&bin, b"hello\x00world").unwrap();
    assert!(is_binary(&bin));
}

#[test]
fn is_binary_allows_text() {
    let dir = tempfile::tempdir().unwrap();
    let txt = dir.path().join("text.txt");
    fs::write(&txt, "hello world").unwrap();
    assert!(!is_binary(&txt));
}

#[cfg(unix)]
#[test]
fn symlink_escape_rejected() {
    let dir = tempfile::tempdir().unwrap();
    // Create a symlink inside the sandbox pointing outside
    let link = dir.path().join("escape");
    std::os::unix::fs::symlink("/tmp", &link).unwrap();
    let result = resolve_sandboxed_path(dir.path(), "escape");
    assert!(result.is_err());
}

#[test]
fn backslash_dotdot_does_not_escape() {
    // On Unix, backslashes are literal filename characters, not separators.
    // On Windows, canonicalize + starts_with catches the escape.
    let dir = tempfile::tempdir().unwrap();
    let child = dir.path().join("sub");
    fs::create_dir(&child).unwrap();
    let result = resolve_sandboxed_path(&child, r"..\..\..\etc\passwd");
    // Must either error or resolve inside the sandbox — never outside.
    match result {
        Err(_) => {} // expected: path doesn't exist (treated as literal or escaped)
        Ok(p) => assert!(p.path.starts_with(child.canonicalize().unwrap())),
    }
}

#[test]
fn whitespace_only_path_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let result = resolve_sandboxed_path(dir.path(), "   ");
    assert!(result.is_err());
}

#[test]
fn dot_resolves_to_sandbox_root() {
    let dir = tempfile::tempdir().unwrap();
    let resolved = resolve_sandboxed_path(dir.path(), ".").unwrap().path;
    assert_eq!(resolved, dir.path().canonicalize().unwrap());
}
