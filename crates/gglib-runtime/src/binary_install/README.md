# Binary Install

<!-- module-docs:start -->

Installing a server binary from a GitHub release, for any product gglib
runs.

A product describes itself once, as a `ReleaseSpec`: its repository, the
release it pins and the environment variable that overrides the pin, the
directory its archive is downloaded to, where a tar.gz holds its files, how
many assets may match a platform, and which archive members it installs. A
platform's pick is a `PrebuiltTarget`: an `AssetMatcher` (every `contains`
part, and an optional `ends_with`), the description the record keeps, the
members that must be there, and where they go. `install_prebuilt` runs the
pipeline from `FetchRelease` to `Verify`; the caller owns `CheckAvailability`,
because deciding the target is product knowledge.

| File         | What it holds                                                      |
|--------------|--------------------------------------------------------------------|
| `release.rs` | `ReleaseSpec`, the pin and its override, `AssetMatcher`            |
| `fetch.rs`   | the release listing, the archive stream, the CUDA runtime package |
| `extract.rs` | unpacking a zip or tar.gz into the bin directory                   |
| `record.rs`  | `PrebuiltRecord`, the four keys a download writes                  |
| `install.rs` | `install_prebuilt`, the pipeline and its phase events              |

The stream is `LlamaProgressEvent` for every product: the frames are the
same, and the TypeScript union that types them is kept by hand.
`InstallPhase::label_for(product)` words a phase for the product being
installed; `label()` is llama.cpp's.

Nothing here prints. Rate and ETA are measured here so that no surface has
to derive them.

<!-- module-docs:end -->
