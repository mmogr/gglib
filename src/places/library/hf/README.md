# hf

<!-- module-docs:start -->

What the library's one search does on Enter. The field filters the library
as it is typed and asks Hugging Face nothing; Enter (or the hint row under
the groups) searches the Hub, opens an exact `user/repo` in the inspector,
or offers a `user/repo:QUANT` download as the Hugging Face group's first
row. With the field empty after it took focus, the group holds the model
`gglib up` would suggest for this machine.

Every figure on a row is the daemon's: the summary's parameter count and
downloads, its reading of whether the repo calls tools, and the
suggestion's rationale and the bytes it was sized by.

| File | Role |
|------|------|
| `useLibrarySearch.ts` | The field's text and what Enter made of it (`enterAction`, from `utils/modelSearchParser`), the scope its first focus sets, and opening a named repo, whose answer is dropped after an edit, a later Enter or a library pick (`cancelOpen`) |
| `useHuggingFaceSearch.ts` | The Hub search: kind, sort, the pages so far and the cursor after them; a kind or sort change asks again, and an overtaken answer is dropped |
| `HfGroup.tsx` | The Hugging Face group: `[Chat · Image]` and `Sort ▾` over a search, the suggestion (its budget in the daemon's words), the direct download, the repos as rows with More results, and the empty and failed lines |

<!-- module-docs:end -->
