---
name: gglib-web-checks-in-worktrees
description: "2026-09-18: in a gglib worktree with node_modules symlinked, `tsc -b` can check nothing (the build info is shared), so force it; and never run Prettier — gglib has no config and it rewrites files wholesale"
metadata: 
  node_type: memory
  type: project
  originSessionId: 6a1728d6-9fc6-4d48-8318-114d8607e481
  modified: 2026-09-18T10:10:22.432Z
---

Two traps in gglib's frontend checks, both found writing #1052 B2 (PR #1095)
in a worktree whose `node_modules` is a symlink to `~/.local/src/gglib`'s:

**`tsc -b` can check nothing.** gglib's tsconfigs keep their build info in
`node_modules/.tmp/*.tsbuildinfo` (`tsconfig.app.json`, `.node.json`,
`.test.json`). With `node_modules` symlinked, every worktree shares those
files. `tsc -b` compares mtimes, so after any other worktree's run it can say
"up to date because newest input … is older than output" and exit 0 in one
second without reading the new code. `make typecheck-web` is `npm run
typecheck` is `tsc -b`, so the gate's pass meant nothing. A reviewer (R-gg6)
proved it with `npx tsc -b --verbose`.
- **Fix:** run `npx tsc -b --force` in the dev check (9 s at B2's c8, against 1
  s unforced), or `tsc -p <each tsconfig> --tsBuildInfoFile <a private path>`.
  A 1-second type check is the tell.

**Prettier is not gglib's formatter.** The repo has no Prettier config.
Running it on four TS files rewrote their quoting and wrapping throughout, and
the change had to be reverted and redone by hand. `lint-web` (ESLint) is the
check; match the surrounding file's style by hand.

**Why:** both look like passing checks while doing something other than what
the log says: one checks nothing, and the other reformats everything.

**How to apply:** force the type check in every per-commit script; never run
`prettier --write` (or an editor's format-on-save) in gglib.

Related: [[a-dev-check-log-names-its-tree]], [[cargo-shared-target-stale-tests]], [[a-worktree-keeps-its-build-output-forever]].
