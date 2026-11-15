#!/usr/bin/env python3
"""The tests each workspace member compiles only on Unix may not grow.

Usage: ./scripts/check_unix_only_tests.py [--update | --list | --self-test]

A test that Windows never compiles leaves the code it covers untested there.
CONTRIBUTING's "A test gated to Unix" says when one is right: when the code
under test is itself Unix-only. This check counts them, per member, against
scripts/unix-only-tests-baseline.txt, so that a new one is a decision the
baseline's diff shows, never an accident.

What is counted. A test is a `fn` under an attribute whose path ends in
`test`: `#[test]`, `#[tokio::test]`, `#[tokio::test(flavor = …)]`. It counts
when Windows does not compile it or does not run it, which is when any of
these holds:

  * an attribute on it is `#[cfg(P)]` with P false on Windows, or
    `#[cfg_attr(P, ignore …)]` with P true there, above or below the test
    attribute;
  * an inline `mod … { }` it is in has such a `#[cfg]`, or its own
    `#![cfg(P)]`;
  * its file is gated: the file starts `#![cfg(P)]` with P false on Windows,
    or every `mod x;` that loads it has such a `#[cfg]` or is in a gated
    file or gated inline `mod`. A `mod x;` loads `x.rs` or `x/mod.rs` beside
    a mod-rs file (lib.rs, main.rs, mod.rs, build.rs, a crate root under
    tests/, benches/, examples/ or src/bin/, or a file a `#[path]` loads),
    or under a directory named after any other file, or the file its
    `#[path = "…"]` names.

P is read on Windows' terms, as true, false or unknown: `windows`,
`target_family = "windows"`, `target_os = "windows"` and `test` are true;
`unix`, any other `target_family` and any other `target_os` are false;
anything else (features, `target_env`, `target_arch`, `debug_assertions`) is
unknown. `not`, `all` and `any` combine them, so `not(windows)`,
`target_os = "macos"` and `all(test, unix)` are false, `not(unix)` is true,
and `any(unix, feature = "x")` is unknown. Only a definite answer counts.

The files are the `.rs` files git lists in the working tree, tracked or
untracked and not ignored, under a member's directory. Comments, string and
character literals are read as such, so a gate written inside one is not.

What it does NOT count, so that nobody reads more into a pass than is there:

  * a test that compiles on Windows and returns early or skips its body
    there (`if cfg!(windows) { return; }`, a `#[cfg]` on a statement);
  * a test whose gate is unknown on Windows (a feature, an architecture);
  * a test an unconditional `#[ignore]` skips everywhere;
  * doctests, tests a macro generates, and tests in a `.rs` file under no
    member (such as crates/build_common.rs, included by build scripts);
  * a `mod x;` whose path a `cfg_attr` chooses, or one inside a macro.

A count above its row fails and names the member and its files. A count
below passes. `--update` writes every row as its count, raising as well as
lowering: a raise belongs only in a change that adds a test of Unix-only
code, with the reason on its gate. A baseline row listed twice, a count
that is not a number and a row naming no member each fail the check, and
`--update` then writes nothing. `--list` prints each file's count.

The self-test runs before every check, and `--self-test` runs it alone: a
fixture repository whose counts are known, planted with each form above
and with the forms that must not count (a `not(unix)` gate, an unknown gate,
a gated helper with no test attribute, gates in comments and strings,
braces in string and character literals, an ignored file and a file under
no member), checked for its exact counts and for each way a baseline must
fail. Its git reads no global or system config.

Exit codes: 0 pass, 1 a count rose, a row is bad or the self-test failed,
2 usage.
"""

from __future__ import annotations

import os
import re
import subprocess
import sys
import tempfile
import tomllib
from typing import Dict, List, NamedTuple, Optional, Set, Tuple

REPO = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
BASELINE = os.path.join(REPO, "scripts", "unix-only-tests-baseline.txt")

TOKEN = re.compile(
    r"""
      (?P<ws>\s+)
    | (?P<line_comment>//[^\n]*)
    | (?P<block_comment>/\*)
    | (?P<raw_str>(?:br|cr|r)(?P<hashes>\#*)")
    | (?P<str>(?:b|c)?"(?:[^"\\]|\\.)*")
    | (?P<char>b?'(?:[^'\\\n]|\\(?:x[0-9a-fA-F]{2}|u\{[0-9a-fA-F]{1,6}\}|.))')
    | (?P<lifetime>'[A-Za-z_][A-Za-z0-9_]*)
    | (?P<ident>r\#[A-Za-z_][A-Za-z0-9_]*|[A-Za-z_][A-Za-z0-9_]*)
    | (?P<num>[0-9][0-9A-Za-z_]*)
    | (?P<punct>.)
    """,
    re.X | re.S,
)

MOD_RS = {"lib.rs", "main.rs", "mod.rs", "build.rs"}
ROOT_DIRS = {"tests", "benches", "examples"}


class Tok(NamedTuple):
    kind: str  # "ident", "str" or "punct"
    text: str
    line: int


class Decl(NamedTuple):
    name: str
    path: Optional[str]  # its `#[path]`, if any
    inline: Tuple[str, ...]  # the inline modules it is declared in
    gated: bool  # its own gate, or an inline module's


class Parsed(NamedTuple):
    gated: bool  # the file's own `#![cfg]`
    decls: Tuple[Decl, ...]
    tests: Tuple[Tuple[int, str, bool], ...]  # (line, name, gated where it stands)


def tokens(src: str) -> List[Tok]:
    out: List[Tok] = []
    pos, line = 0, 1
    while pos < len(src):
        m = TOKEN.match(src, pos)
        end = m.end()
        if m.group("block_comment"):
            depth, end = 1, pos + 2
            while depth and end < len(src):
                if src.startswith("/*", end):
                    depth, end = depth + 1, end + 2
                elif src.startswith("*/", end):
                    depth, end = depth - 1, end + 2
                else:
                    end += 1
        elif m.group("raw_str"):
            close = '"' + m.group("hashes")
            found = src.find(close, end)
            body_end = found if found >= 0 else len(src)
            out.append(Tok("str", src[end:body_end], line))
            end = body_end + len(close) if found >= 0 else len(src)
        elif m.group("str"):
            text = m.group("str")
            out.append(Tok("str", text[text.index('"') + 1 : -1], line))
        elif m.group("ident"):
            out.append(Tok("ident", m.group("ident").removeprefix("r#"), line))
        elif m.group("punct"):
            out.append(Tok("punct", m.group("punct"), line))
        line += src.count("\n", pos, end)
        pos = end
    return out


def closing(toks: List[Tok], i: int, opener: str, closer: str) -> int:
    """The index of the token closing the `opener` at `i`."""
    depth = 0
    for j in range(i, len(toks)):
        if toks[j].kind == "punct" and toks[j].text == opener:
            depth += 1
        elif toks[j].kind == "punct" and toks[j].text == closer:
            depth -= 1
            if depth == 0:
                return j
    return len(toks) - 1


def is_punct(toks: List[Tok], i: int, text: str) -> bool:
    return i < len(toks) and toks[i].kind == "punct" and toks[i].text == text


def attr_path(attr: List[Tok]) -> Tuple[str, int]:
    """An attribute's path, `a::b`, and the index just after it."""
    parts, i = [], 0
    while i < len(attr) and attr[i].kind == "ident":
        parts.append(attr[i].text)
        if is_punct(attr, i + 1, ":") and is_punct(attr, i + 2, ":"):
            i += 3
        else:
            i += 1
            break
    return "::".join(parts), i


def predicate(toks: List[Tok], i: int) -> Tuple[Optional[bool], int]:
    """Reads the cfg predicate at `i` on Windows' terms."""
    if i >= len(toks) or toks[i].kind != "ident":
        return None, i + 1
    name = toks[i].text
    if is_punct(toks, i + 1, "=") and i + 2 < len(toks) and toks[i + 2].kind == "str":
        value = toks[i + 2].text
        if name in ("target_family", "target_os"):
            return value == "windows", i + 3
        return None, i + 3
    if is_punct(toks, i + 1, "("):
        end = closing(toks, i + 1, "(", ")")
        args, j = [], i + 2
        while j < end:
            value, j = predicate(toks, j)
            args.append(value)
            if is_punct(toks, j, ","):
                j += 1
        if name == "not" and len(args) == 1:
            value = None if args[0] is None else not args[0]
        elif name == "all":
            value = False if False in args else (None if None in args else True)
        elif name == "any":
            value = True if True in args else (None if None in args else False)
        else:
            value = None
        return value, end + 1
    return {"windows": True, "test": True, "unix": False}.get(name), i + 1


def off_on_windows(attrs: List[List[Tok]]) -> bool:
    """Whether these attributes keep their item from compiling, or a test
    from running, on Windows."""
    for attr in attrs:
        path, i = attr_path(attr)
        if path == "cfg" and is_punct(attr, i, "("):
            if predicate(attr, i + 1)[0] is False:
                return True
        elif path == "cfg_attr" and is_punct(attr, i, "("):
            value, j = predicate(attr, i + 1)
            rest = {t.text for t in attr[j:] if t.kind == "ident"}
            if value is True and "ignore" in rest:
                return True
    return False


def parse(src: str) -> Parsed:
    toks = tokens(src)
    # Each brace scope: (gated, the inline module's name or None).
    scopes: List[Tuple[bool, Optional[str]]] = [(False, None)]
    pending: List[List[Tok]] = []
    file_gated = False
    decls: List[Decl] = []
    tests: List[Tuple[int, str, bool]] = []
    i = 0
    while i < len(toks):
        tok = toks[i]
        inner = is_punct(toks, i + 1, "!")
        if is_punct(toks, i, "#") and is_punct(toks, i + 1 + inner, "["):
            start = i + 1 + inner
            end = closing(toks, start, "[", "]")
            attr = toks[start + 1 : end]
            if inner and off_on_windows([attr]):
                if len(scopes) == 1:
                    file_gated = True
                else:
                    scopes[-1] = (True, scopes[-1][1])
            elif not inner:
                pending.append(attr)
            i = end + 1
            continue
        gated = scopes[-1][0]
        if tok.kind == "ident" and tok.text == "mod" and i + 1 < len(toks) and toks[i + 1].kind == "ident":
            name = toks[i + 1].text
            mod_gated = gated or off_on_windows(pending)
            if is_punct(toks, i + 2, "{"):
                scopes.append((mod_gated, name))
            elif is_punct(toks, i + 2, ";"):
                path = None
                for attr in pending:
                    p, j = attr_path(attr)
                    if p == "path" and is_punct(attr, j, "=") and j + 1 < len(attr):
                        path = attr[j + 1].text
                inline = tuple(n for _, n in scopes[1:] if n is not None)
                decls.append(Decl(name, path, inline, mod_gated))
            pending = []
            i += 3
            continue
        if tok.kind == "ident" and tok.text == "fn":
            if any(attr_path(a)[0].split("::")[-1] == "test" for a in pending):
                name = toks[i + 1].text if i + 1 < len(toks) else "?"
                tests.append((tok.line, name, gated or off_on_windows(pending)))
            pending = []
        elif is_punct(toks, i, "{"):
            scopes.append((gated, None))
            pending = []
        elif is_punct(toks, i, "}"):
            if len(scopes) > 1:
                scopes.pop()
            pending = []
        elif is_punct(toks, i, ";"):
            pending = []
        i += 1
    return Parsed(file_gated, tuple(decls), tuple(tests))


def git_env() -> Dict[str, str]:
    """The environment without the variables that would point git at
    another repository, as a hook in a linked worktree exports them."""
    env = dict(os.environ)
    for name in ("GIT_DIR", "GIT_INDEX_FILE", "GIT_WORK_TREE", "GIT_OBJECT_DIRECTORY",
                 "GIT_ALTERNATE_OBJECT_DIRECTORIES", "GIT_COMMON_DIR", "GIT_PREFIX"):
        env.pop(name, None)
    return env


def rs_files(root: str) -> List[str]:
    listed = subprocess.run(
        ["git", "-C", root, "ls-files", "-z", "--cached", "--others", "--exclude-standard", "--deduplicate"],
        check=True, capture_output=True, env=git_env(),
    ).stdout.decode().split("\0")
    return sorted(f for f in listed if f.endswith(".rs") and os.path.isfile(os.path.join(root, f)))


def members(root: str) -> List[str]:
    with open(os.path.join(root, "Cargo.toml"), "rb") as f:
        return list(tomllib.load(f).get("workspace", {}).get("members", []))


def is_mod_rs(path: str) -> bool:
    parts = path.split("/")
    return (
        parts[-1] in MOD_RS
        or (len(parts) > 1 and parts[-2] in ROOT_DIRS)
        or (len(parts) > 2 and parts[-3:-1] == ["src", "bin"])
    )


def loaded(path: str, decl: Decl, mod_rs: bool) -> List[str]:
    """The files `decl`, declared in `path`, can load."""
    here = os.path.dirname(path)
    owner = here if mod_rs else os.path.join(here, os.path.basename(path)[: -len(".rs")])
    if decl.path is not None:
        base = here if not decl.inline else os.path.join(owner, *decl.inline)
        return [os.path.normpath(os.path.join(base, decl.path))]
    base = os.path.join(owner, *decl.inline)
    return [os.path.normpath(os.path.join(base, decl.name + ".rs")),
            os.path.normpath(os.path.join(base, decl.name, "mod.rs"))]


def unix_only(root: str, files: List[str]) -> Dict[str, List[Tuple[int, str]]]:
    """Each file's tests compiled only on Unix, as (line, name)."""
    parsed: Dict[str, Parsed] = {}
    for f in files:
        with open(os.path.join(root, f), encoding="utf-8", errors="replace") as fh:
            parsed[f] = parse(fh.read())
    listed = set(files)
    path_loaded: Set[str] = set()
    for f, p in parsed.items():
        for d in p.decls:
            if d.path is not None:
                path_loaded.update(loaded(f, d, True))
    # file -> [(declaring file, the declaration's own gate)]
    loaders: Dict[str, List[Tuple[str, bool]]] = {}
    for f, p in parsed.items():
        mod_rs = is_mod_rs(f) or f in path_loaded
        for d in p.decls:
            for target in loaded(f, d, mod_rs):
                if target in listed:
                    loaders.setdefault(target, []).append((f, d.gated))

    memo: Dict[str, bool] = {}

    def file_gated(f: str, seen: Tuple[str, ...] = ()) -> bool:
        if f in memo:
            return memo[f]
        if f in seen:
            return False
        via = loaders.get(f, [])
        gated = parsed[f].gated or (
            bool(via) and all(g or file_gated(parent, seen + (f,)) for parent, g in via)
        )
        memo[f] = gated
        return gated

    out: Dict[str, List[Tuple[int, str]]] = {}
    for f, p in parsed.items():
        whole = file_gated(f)
        hits = [(line, name) for line, name, gated in p.tests if gated or whole]
        if hits:
            out[f] = hits
    return out


def counts(root: str) -> Tuple[List[Tuple[str, int]], Dict[str, List[Tuple[int, str]]]]:
    files = rs_files(root)
    found = unix_only(root, files)
    rows = []
    for m in members(root):
        rows.append((m, sum(len(v) for f, v in found.items() if f.startswith(m + "/"))))
    return rows, found


def read_baseline(path: str) -> Tuple[Dict[str, int], List[str]]:
    rows: Dict[str, int] = {}
    problems: List[str] = []
    try:
        with open(path, encoding="utf-8") as f:
            lines = f.read().splitlines()
    except FileNotFoundError:
        return rows, [f"❌ missing baseline: {path}"]
    for line in lines:
        if not line.strip() or line.startswith("#"):
            continue
        fields = line.split()
        if len(fields) != 2 or not fields[1].isdigit():
            problems.append(f"❌ baseline row {line!r} is not `<member> <count>`")
        elif fields[0] in rows:
            problems.append(f"❌ baseline lists {fields[0]} twice")
        else:
            rows[fields[0]] = int(fields[1])
    return rows, problems


def check(root: str, baseline: str, update: bool = False) -> Tuple[bool, List[str]]:
    rows, found = counts(root)
    base, problems = read_baseline(baseline)
    out = list(problems)
    names = [m for m, _ in rows]
    for m in base:
        if m not in names:
            out.append(f"❌ baseline names {m}, which is not a workspace member")
    if not rows or not rs_files(root):
        out.append(f"❌ found {len(rows)} members and no .rs files: nothing was checked")
    stale = bool(out)
    for m, n in rows:
        if not update and n > base.get(m, 0):
            out.append(f"❌ {m}: {n} tests compiled only on Unix, baseline {base.get(m, 0)}")
            for f in sorted(found):
                if f.startswith(m + "/"):
                    out.append(f"   {len(found[f]):>3} {f}")
    if update and not stale:
        with open(baseline, "w", encoding="utf-8") as fh:
            fh.write("# <member> <tests compiled only on Unix>\n")
            fh.write("# Written by scripts/check_unix_only_tests.py --update.\n")
            for m, n in rows:
                fh.write(f"{m} {n}\n")
    return not out, out


FIXTURE = {
    "Cargo.toml": '[workspace]\nmembers = ["good", "other"]\n',
    "other/Cargo.toml": '[package]\nname = "other"\n',
    "other/src/lib.rs": "#[test]\nfn runs_everywhere() {}\n",
    "good/Cargo.toml": '[package]\nname = "good"\n',
    # 8 here: a, b, c, d, e, i, and k and m in the inline module.
    "good/src/lib.rs": r'''
#[cfg(unix)]
#[test]
fn a() {}
#[test]
#[cfg(unix)]
fn b() { let _ = '{'; let _ = "}"; let _ = r#"}"#; }
#[cfg(not(windows))]
#[tokio::test]
async fn c() {}
#[cfg(target_family = "unix")]
#[tokio::test(flavor = "multi_thread")]
async fn d() {}
#[cfg(all(test, target_os = "macos", feature = "x"))]
#[test]
fn e() {}
#[cfg(not(unix))]
#[test]
fn f() {}
#[cfg(any(unix, windows))]
#[test]
fn g() {}
#[cfg(any(unix, feature = "x"))]
#[test]
fn h() {}
#[cfg_attr(windows, ignore = "no sh there")]
#[test]
fn i() {}
#[cfg_attr(unix, ignore)]
#[test]
fn j() {}
#[cfg(unix)]
fn helper() {}
#[test]
fn plain() {}
// #[cfg(unix)] #[test] fn in_a_comment() {}
/* #[cfg(unix)] /* nested */ #[test] fn in_a_block_comment() {} */
const S: &str = "#[cfg(unix)] #[test] fn in_a_string() {}";
#[cfg(all(test, unix))]
pub(crate) mod inline {
    #[test]
    fn k() {}
    #[cfg(unix)]
    #[test]
    fn m() {}
}
#[cfg(test)]
mod open {
    #[test]
    fn n() {}
}
#[cfg(all(test, unix))]
#[path = "gated_tests.rs"]
mod gated;
#[cfg(unix)]
mod sub;
#[cfg(test)]
mod inner_gated;
#[cfg(test)]
mod not_gated;
#[cfg(test)]
#[path = "elsewhere/twice_tests.rs"]
mod twice;
''',
    # 2, loaded by a gated `#[path]`.
    "good/src/gated_tests.rs": "#[test]\nfn p() {}\n#[tokio::test]\nasync fn q() {}\n",
    # 1, in a gated module's file, and 1 in the file it loads in turn.
    "good/src/sub.rs": "mod deeper;\n#[test]\nfn r() {}\n",
    "good/src/sub/deeper.rs": "#[test]\nfn s() {}\n",
    # 1, gated by its own first line.
    "good/src/inner_gated.rs": "#![cfg(unix)]\n#[test]\nfn t() {}\n",
    # 0: not gated anywhere, and `not(unix)` is true on Windows.
    "good/src/not_gated.rs": "#![cfg(not(unix))]\n#[test]\nfn u() {}\n",
    # 0: loaded twice, once ungated.
    "good/src/elsewhere/twice_tests.rs": "#[test]\nfn v() {}\n",
    # 1: a test crate gated whole.
    "good/tests/it.rs": "#![cfg(unix)]\n#[test]\nfn w() {}\n#[cfg(windows)]\nmod win;\n",
    # 0: ignored, and under no member.
    "good/target/gen.rs": "#![cfg(unix)]\n#[test]\nfn x() {}\n",
    "shared.rs": "#![cfg(unix)]\n#[test]\nfn y() {}\n",
    ".gitignore": "target/\n",
}
FIXTURE["good/src/lib.rs"] += "#[path = \"elsewhere/twice_tests.rs\"]\n#[cfg(unix)]\nmod twice_again;\n"
EXPECTED = [("good", 14), ("other", 0)]


def self_test() -> bool:
    def fail(why: str, out: List[str]) -> bool:
        print(f"❌ self-test: {why}")
        for line in out:
            print(f"     {line}")
        return False

    with tempfile.TemporaryDirectory() as root:
        for path, text in FIXTURE.items():
            os.makedirs(os.path.dirname(os.path.join(root, path)) or root, exist_ok=True)
            with open(os.path.join(root, path), "w", encoding="utf-8") as f:
                f.write(text)
        env = git_env() | {"GIT_CONFIG_GLOBAL": os.devnull, "GIT_CONFIG_NOSYSTEM": "1", "XDG_CONFIG_HOME": root}
        subprocess.run(["git", "-C", root, "init", "-q", "--template="], check=True, env=env)
        # One file tracked, the rest untracked: both are listed.
        subprocess.run(["git", "-C", root, "add", "good/src/lib.rs"], check=True, env=env)
        saved = {k: os.environ.get(k) for k in ("GIT_CONFIG_GLOBAL", "GIT_CONFIG_NOSYSTEM", "XDG_CONFIG_HOME")}
        os.environ.update({k: env[k] for k in saved})
        try:
            return run_fixture(root, fail)
        finally:
            for k, v in saved.items():
                if v is None:
                    os.environ.pop(k, None)
                else:
                    os.environ[k] = v


def run_fixture(root: str, fail) -> bool:
    baseline = os.path.join(root, "baseline")

    def write(text: str) -> None:
        with open(baseline, "w", encoding="utf-8") as f:
            f.write(text)

    rows, found = counts(root)
    if rows != EXPECTED:
        got = [f"{f}: {', '.join(n for _, n in v)}" for f, v in sorted(found.items())]
        return fail(f"counted {rows}, not {EXPECTED}", got)

    write("good 14\nother 0\n")
    ok, out = check(root, baseline)
    if not ok:
        return fail("the fixture at its baseline failed", out)
    write("good 20\n")
    ok, out = check(root, baseline)
    if not ok:
        return fail("a count below its baseline failed", out)
    write("good 13\nother 0\n")
    ok, out = check(root, baseline)
    if ok or "❌ good: 14 tests compiled only on Unix, baseline 13" not in out \
            or "     8 good/src/lib.rs" not in out:
        return fail("a count above its baseline passed, or its files went unnamed", out)
    for text, why in [
        ("good 14\nother 0\ngood 14\n", "❌ baseline lists good twice"),
        ("good x\n", "❌ baseline row 'good x' is not `<member> <count>`"),
        ("good 14\ngone 0\n", "❌ baseline names gone, which is not a workspace member"),
    ]:
        write(text)
        ok, out = check(root, baseline)
        if ok or why not in out:
            return fail(f"a baseline that must fail with {why!r} did not", out)
        before = text
        ok, out = check(root, baseline, update=True)
        with open(baseline, encoding="utf-8") as f:
            if ok or f.read() != before:
                return fail(f"--update over {why!r} passed, or rewrote the baseline", out)
    for start in ("good 20\nother 3\n", "good 2\n"):
        write(start)
        ok, out = check(root, baseline, update=True)
        with open(baseline, encoding="utf-8") as f:
            rows_written = [l for l in f.read().splitlines() if not l.startswith("#")]
        if not ok or rows_written != ["good 14", "other 0"]:
            return fail(f"--update from {start!r} did not write the counts", out + rows_written)

    os.remove(os.path.join(root, ".git", "index"))
    with open(os.path.join(root, ".gitignore"), "w", encoding="utf-8") as f:
        f.write("*\n")
    write("good 14\nother 0\n")
    ok, out = check(root, baseline)
    if ok or not any("nothing was checked" in line for line in out):
        return fail("a tree with no .rs file listed passed", out)

    print(
        "✓ self-test: 14 tests counted in a fixture of each gated form (cfg above and below the "
        "test attribute, not(windows), target_family, a target_os, an all(), cfg_attr ignore, an "
        "inline module, a gated #[path] and plain mod chain, #![cfg] in a module and a test crate), "
        "none for not(unix), any(), an unknown gate, a gated helper, a file loaded once ungated, "
        "comments, strings, an ignored file or one under no member; a rise, a duplicate, a bad "
        "and a stale row and an empty listing refused; --update raises and lowers, and writes "
        "nothing over a bad row"
    )
    return True


def main(argv: List[str]) -> int:
    arg = argv[1] if len(argv) > 1 else ""
    if len(argv) > 2 or arg not in ("", "--update", "--list", "--self-test"):
        print(f"usage: {argv[0]} [--update | --list | --self-test]", file=sys.stderr)
        return 2
    if not self_test():
        print("The check cannot verify its own detection, so its verdict means nothing.")
        return 1
    if arg == "--self-test":
        return 0
    if arg == "--list":
        _, found = counts(REPO)
        for f in sorted(found):
            print(f"{len(found[f]):>4} {f}")
        print(f"{sum(len(v) for v in found.values()):>4} in all")
        return 0
    ok, out = check(REPO, BASELINE, update=arg == "--update")
    for line in out:
        print(line)
    if arg == "--update":
        print(f"✅ baseline updated: {BASELINE}" if ok else "❌ baseline not updated")
        return 0 if ok else 1
    if not ok:
        print(
            "\nThe ❌ lines above say what failed. A test gated to Unix needs a twin that runs "
            "on Windows (gglib_test_proc::install makes the fake program), unless the code it "
            "tests is itself Unix-only. Then say so on its gate, and raise the row with "
            "./scripts/check_unix_only_tests.py --update in the same change."
        )
        return 1
    print("✅ no member compiles more tests only on Unix than its baseline")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
