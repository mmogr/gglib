#!/usr/bin/env python3
"""Rebase a reviewed PR branch onto origin/main and push it only when the reviewed patch is unchanged.

Usage: rebase_pr.py <worktree> <branch>

The one conflict it resolves by itself: a README.md in which main's only change was deleting the
generated `<!-- module-table:start -->...<!-- module-table:end -->` block (gglib #1130). There the
result is the branch's README with that block removed. Anything else aborts and pushes nothing.

After the rebase it checks, before pushing:
- every non-README file's patch is byte-identical to the reviewed one (range-diff with a pathspec);
- every README the branch touched equals the reviewed version with module-table blocks removed;
- every commit is authored and committed by Matt O'Grady <sewing.wader8c@icloud.com>.
"""
import re, subprocess, sys

ID = ("Matt O'Grady", "sewing.wader8c@icloud.com")
BLOCK = re.compile(r"\n?<!-- module-table:start -->.*?<!-- module-table:end -->\n?", re.S)


def git(wt, *a, check=True, inp=None):
    r = subprocess.run(["git", "-C", wt, *a], capture_output=True, text=True, input=inp)
    if check and r.returncode:
        raise RuntimeError(f"git {' '.join(a)}: {r.stderr.strip()}")
    return r.stdout


WRAP = re.compile(r"\n?<details>\s*<summary><h2>Modules</h2></summary>\s*</details>\n?", re.S)


def strip(s):
    # #1130 removed each module-table block and the <details> "Modules" wrapper it sat in
    return WRAP.sub("\n", BLOCK.sub("\n", s)) if s is not None else None


def show(wt, spec):
    r = subprocess.run(["git", "-C", wt, "show", spec], capture_output=True, text=True)
    return r.stdout if r.returncode == 0 else None


def norm(s):
    # the block sat between two blank-line-separated sections; compare with runs of blank lines collapsed
    return re.sub(r"\n{3,}", "\n\n", s).strip() + "\n" if s is not None else None


def main():
    wt, br = sys.argv[1], sys.argv[2]
    # optional: --from <old-parent> replays only old-parent..branch onto origin/main (a stack child whose parent squash-merged)
    frm = sys.argv[sys.argv.index("--from") + 1] if "--from" in sys.argv else None
    git(wt, "fetch", "-q", "origin")
    if git(wt, "status", "--porcelain").strip():
        print(f"REFUSE: {wt} is dirty"); return 1
    if git(wt, "rev-parse", "--abbrev-ref", "HEAD").strip() != br:
        git(wt, "switch", "-q", br)
    old = git(wt, "rev-parse", "HEAD").strip()
    main_sha = git(wt, "rev-parse", "origin/main").strip()
    oldbase = git(wt, "rev-parse", frm).strip() if frm else git(wt, "merge-base", old, "origin/main").strip()
    if oldbase == main_sha:
        print(f"UP-TO-DATE {br}"); return 0
    env_id = ["-c", f"user.name={ID[0]}", "-c", f"user.email={ID[1]}"]
    cmd = ["rebase", "-q", "--onto", "origin/main", oldbase] if frm else ["rebase", "-q", "origin/main"]
    r = subprocess.run(["git", "-C", wt, *env_id, *cmd], capture_output=True, text=True)
    resolved = []
    while r.returncode != 0:
        conflicted = [p for p in git(wt, "diff", "--name-only", "--diff-filter=U").split() if p]
        if not conflicted:
            git(wt, "rebase", "--abort", check=False); print(f"CONFLICT {br}: rebase stopped without a file conflict: {r.stderr.strip()[:300]}"); return 3
        for p in conflicted:
            base, ours, theirs = show(wt, f":1:{p}"), show(wt, f":2:{p}"), show(wt, f":3:{p}")
            if not p.endswith("README.md") or base is None or ours is None or theirs is None or norm(strip(base)) != norm(ours):
                git(wt, "rebase", "--abort", check=False)
                print(f"CONFLICT {br}: {p} is not a module-table-only conflict; rebase aborted, nothing pushed"); return 3
            with open(f"{wt}/{p}", "w") as fh:
                fh.write(norm(strip(theirs)))
            git(wt, "add", p)
            resolved.append(p)
        r = subprocess.run(["git", "-C", wt, *env_id, "-c", "core.editor=true", "rebase", "--continue"], capture_output=True, text=True)
    new = git(wt, "rev-parse", "HEAD").strip()
    # 1. non-README patches unchanged
    rd = git(wt, "range-diff", f"{oldbase}..{old}", f"origin/main..{new}", "--", ".", ":(exclude,glob)**/README.md", ":(exclude)README.md")
    if any(not re.match(r"^\d+: +[0-9a-f]+ = \d+: +[0-9a-f]+ ", l) for l in rd.splitlines() if l.strip()):
        git(wt, "reset", "-q", "--hard", old); print(f"CHANGED {br}: non-README patch differs; not pushed\n{rd[:2000]}"); return 4
    # 2. each README's changed lines are the reviewed ones, minus module-table lines (main may have changed the file too)
    TABLE = re.compile(r"^[+-](\| \[`|\|[-| ]+\|\s*$|\| (File|Module|Directory) \||<!-- module-table|<details>|<summary><h2>Modules|</details>|\s*$)")
    def changed(a, b, path):
        d = git(wt, "diff", "-U0", a, b, "--", path)
        return sorted(l for l in d.splitlines() if l[:1] in "+-" and not l.startswith(("+++", "---")) and not TABLE.match(l))
    for p in [x for x in git(wt, "diff", "--name-only", f"{oldbase}..{old}").split() if x.endswith("README.md")]:
        if changed(oldbase, old, p) != changed("origin/main", new, p):
            git(wt, "reset", "-q", "--hard", old); print(f"CHANGED {br}: {p}'s changed lines differ from the reviewed ones (table lines aside); not pushed"); return 4
    # 3. identity
    bad = [l for l in git(wt, "log", "--format=%an|%ae|%cn|%ce", f"origin/main..{new}").splitlines() if l != f"{ID[0]}|{ID[1]}|{ID[0]}|{ID[1]}"]
    if bad:
        git(wt, "reset", "-q", "--hard", old); print(f"REFUSE identity: {bad}"); return 1
    if "--no-push" in sys.argv:
        note = f"; resolved module-table-only conflicts in {', '.join(resolved)}" if resolved else ""
        print(f"REBASED-LOCAL {br} {old[:8]} -> {new[:8]} (non-README patch unchanged{note}; not pushed)")
        return 0
    pr = subprocess.run(["git", "-C", wt, "push", "-q", f"--force-with-lease={br}:{old}", "origin", br], capture_output=True, text=True)
    if pr.returncode:
        print(f"PUSH FAILED {br}: {pr.stderr.strip()}"); return 5
    note = f"; resolved module-table-only conflicts in {', '.join(resolved)}" if resolved else ""
    print(f"REBASED {br} {old[:8]} -> {new[:8]} (non-README patch unchanged{note})")
    return 0


if __name__ == "__main__":
    sys.exit(main())
