# bin

<!-- module-docs:start -->

Two tools built from the crate, neither part of the framework.

`uniffi-bindgen` is the generator of gglib-ffi's Swift binding, header and
framework modulemap. It is built from this crate's own `uniffi`, so the Swift
it writes matches the scaffolding compiled into the library exactly. It is
behind the crate's `bindgen` feature (`required-features`), so a build of the
library for a phone never compiles it or what it depends on.
`scripts/apple/generate-swift.sh` builds and runs it; `make ffi-binding` and
the framework build call that script.

`gglib-ffi-add-model` (`add-model.rs`) adds a GGUF already on disk to a
library's folders, as `gglib model add` adds one on the desktop, and prints
the name it is served under. The binding has no call that does this, since a
phone gets its models by download, and the Swift smoke test
(`scripts/apple/swift-smoke.sh`) needs a model in the library to serve. It is
behind the crate's `smoke` feature.

<!-- module-docs:end -->
