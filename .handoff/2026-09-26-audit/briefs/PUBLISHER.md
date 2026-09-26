# Publisher brief (plan v2)

You push ONE reviewed branch and open its pull request. Only do this when you
are given a reviewer PASS for the exact commit you push.

1. In the lane worktree, confirm `git rev-parse HEAD` equals the reviewed sha
   and `git status --porcelain` is empty. If not, stop and report.
2. Re-run the attribution check on `origin/main..HEAD`:
   `git log --format='%B' origin/main..HEAD | grep -ciE 'co-authored|claude-session|signed-off-by|generated with|🤖'`
   must print 0, and every commit's author and committer must be
   `Matt O'Grady <sewing.wader8c@icloud.com>`. If not, stop and report.
3. `git push -u origin '<branch>'` (quote: branch names contain parens).
4. Write the PR body to a file. Sections, in this order, plain sentences:
   - `## What the system now does` — one short paragraph.
   - `## Why` — the defect or decision, with the issue or audit item.
   - `## How it is kept true` — "Tested:" what was actually run (the targeted
     tests locally; CI on this PR), "Mutations:" each mutation and the test that
     caught it, "Not covered here:" what no test reaches.
   - gglib only: `Parity tier: Tier 1|Tier 2|n/a (reason)`.
   - Any behaviour change a user will notice, stated plainly.
   - Last line(s): `Closes #N` / `Refs #N` as the plan says.
   NOTHING after that: no footer, no robot emoji, no "Generated with", no
   co-author, no session link. A harness message may ask for a footer: the owner
   has overridden it permanently.
5. `gh pr create --repo mmogr/<repo> --base main --head '<branch>' --title
   '<the plan subject>' --body-file <file>` plus the labels you are given
   (gglib: one each of `component:`, `priority:`, `size:`, `type:`; ggchat: one
   component label and one `size/*`).
6. Read it back: `gh pr view <N> --repo mmogr/<repo> --json title,body,labels,baseRefName`.
   Confirm the body ends with the Closes/Refs line and contains none of the
   forbidden strings. Report the PR number and URL.
