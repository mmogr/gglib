//! The Swift generator, as a binary inside the crate it binds.
//!
//! Built from this crate's own `uniffi`, so the generated Swift matches the
//! scaffolding compiled into the library exactly; a separately installed
//! `uniffi-bindgen` would be a second version to keep in step by hand.
//!
//! `uniffi`'s Swift-specific command rather than its generic one: only it
//! writes a `framework module` modulemap (`--xcframework`), which is what a
//! framework's `Modules/module.modulemap` has to declare.
//! `scripts/apple/generate-swift.sh` runs it; it is not shipped.

fn main() {
    uniffi::uniffi_bindgen_swift();
}
