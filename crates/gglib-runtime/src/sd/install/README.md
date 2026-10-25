# install

<!-- module-docs:start -->

Getting stable-diffusion.cpp's `sd-server` onto the machine, and saying what
is there. Its record is `binary_install::record`'s and removing it is
`engine::uninstall`, both shared with llama.cpp. The story, the platform table and the source
build are in [the parent README](../README.md).

| File           | What it holds                                                          |
|----------------|------------------------------------------------------------------------|
| `release.rs`   | the pin `master-948-228c707`, `GGLIB_SD_RELEASE`, the platform table   |
| `pipeline.rs`  | the pre-built install, and the source build of the `sd-server` target  |
| `status.rs`    | `SdStatus`, from the record and the binary's own `--version`           |

<!-- module-docs:end -->
