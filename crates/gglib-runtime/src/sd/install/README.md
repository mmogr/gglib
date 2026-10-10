# install

<!-- module-docs:start -->

Getting stable-diffusion.cpp's `sd-server` onto the machine, saying what is
there, and taking it away. The story, the platform table and the source
build are in [the parent README](../README.md).

| File           | What it holds                                                          |
|----------------|------------------------------------------------------------------------|
| `release.rs`   | the pin `master-948-228c707`, `GGLIB_SD_RELEASE`, the platform table   |
| `pipeline.rs`  | the pre-built install, and the source build of the `sd-server` target  |
| `record.rs`    | `sd-config.json`: a source build's five keys or a download's four      |
| `status.rs`    | `SdStatus`, from the record and the binary's own `--version`           |
| `uninstall.rs` | removing `.sd/`                                                        |

<!-- module-docs:end -->
