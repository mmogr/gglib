# bin

<!-- module-docs:start -->

`uniffi-bindgen`, the generator of gglib-ffi's Swift binding, header and
framework modulemap. It is built from this crate's own `uniffi`, so the Swift
it writes matches the scaffolding compiled into the library exactly.

It is behind the crate's `bindgen` feature (`required-features`), so a build
of the library for a phone never compiles it or what it depends on, and it is
not part of the framework. `scripts/apple/generate-swift.sh` builds and runs
it; `make ffi-binding` and the framework build call that script.

<!-- module-docs:end -->
