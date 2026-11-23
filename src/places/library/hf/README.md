# hf

<!-- module-docs:start -->

What the library's one search does on Enter. The field filters the library
as it is typed and asks Hugging Face nothing; Enter (or the hint row under
the groups) searches the Hub, opens an exact `user/repo` in the inspector,
or offers a `user/repo:QUANT` download as the Hugging Face group's first
row. With the field empty after it took focus, the group holds the model
`gglib up` would suggest for this machine, until it is dismissed: the
dismissal is remembered by repo (`localStorage`, in try/catch), so a later
suggestion of another repo is shown again.

Every figure on a row is the daemon's: the summary's parameter count,
downloads and reading of whether the repo calls tools (from the chat
template the search answer carries, so no row asks the Hub anything of
its own), and the suggestion's rationale and the bytes it was sized by.

A search, a "More results" or a named repo Hugging Face rate limits is
asked again by itself when the limit lifts; until then the place its
answer would be says "Hugging Face's rate limit lifts in 2:14. gglib will
read this again then." (the page's own words, counting down;
`components/status/HubWaitLine`). A newer search, a cleared field, an
edit or leaving the library stops the wait. A first page that waits or
fails shows no rows; a "More results" that waits or fails keeps the rows
already listed and says so under them, and one that failed keeps More
results to try again. A Download (the suggestion's, or a
`user/repo:QUANT`'s) the limit refuses is held as a row in the
Downloading group and queued when it lifts, and its toast says so
(`hooks/useQueueDownload`).

| File | Role |
|------|------|
| `useLibrarySearch.ts` | The field's text and what Enter made of it (`enterAction`, from `utils/modelSearchParser`), the scope its first focus sets, and opening a named repo, read again when the Hub's limit lifts, whose answer and wait are dropped after an edit, a later Enter or a library pick (`cancelOpen`) |
| `useHuggingFaceSearch.ts` | The Hub search: kind, sort, the pages so far and the cursor after them; a kind or sort change asks again, an overtaken answer is dropped, and a request the Hub's limit refused is asked again when it lifts unless a newer one, a clear or an unmount stopped the wait |
| `HfGroup.tsx` | The Hugging Face group: `[Chat · Image]` and `Sort ▾` over a search, the suggestion (its budget in the daemon's words, and Dismiss), the direct download, the repos as rows with More results, and the empty, failed and waiting lines (a More results' under the rows) |

<!-- module-docs:end -->
