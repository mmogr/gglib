---
name: mutation-restore-must-refresh-mtime
description: "2026-09-17: restoring a mutated Rust file with shutil.copy2/cp -p keeps the old mtime, cargo skips the rebuild, and every later mutation runs on the earlier mutation's binary; restore by rewriting the bytes, and start each run from a touched, clean build"
metadata: 
  node_type: memory
  type: project
  originSessionId: 7b04dc95-1936-4c6a-b0d3-9cc5d4879bb7
  modified: 2026-09-17T11:27:36.358Z
---

On 2026-09-17 (gglib #947) a six-mutation runner restored each file with
`shutil.copy2(backup, path)`. `copy2` preserves the backup's **original mtime**,
which is older than cargo's last build of the mutated file, so cargo's
fingerprint read the restored file as unchanged and **kept the mutated
binary**. Every later mutation then ran on top of the earlier ones. The tell:
a mutation that touched only `gglib-proxy/src/metrics.rs` "failed" a test in
`gglib-core` — impossible. All six still printed KILLED, so without reading
*which* tests failed the run would have looked perfect. The first mutation's
result was the only sound one.

**Why:** the digest check after the restore passed (the bytes were right); the
build system's view was wrong. It is [[cargo-shared-target-stale-tests]] in a
new shape: a green/red result read off a binary that was not built from the
tree in front of you.

**How to apply:**
- Restore a mutated file by **rewriting its bytes** (`path.write_bytes(...)`,
  or `cp` without `-p` followed by nothing clever), never `copy2`/`cp -p`/`mv`
  of a backup. Same for shell runners: `cp bak file && touch file`.
- Begin a mutation run by `touch`ing every file any mutation will edit and
  running the unmutated suite once, asserting it passes — so the run starts
  from binaries built from the pristine tree whatever an earlier run left.
- Run the **whole** relevant suite per mutation (`--no-fail-fast`) and list
  every failed test, not just the named one: an impossible failure is how
  contamination shows itself, and the full list is also the honest "killed by"
  line for the commit message.
- Still read the build exit separately from the test exit, and still check the
  worktree diff's hash before and after the run.

Related: [[cargo-shared-target-stale-tests]], [[scripted-edit-must-be-read-back]], [[gglib-adversarial-review-loop]].
