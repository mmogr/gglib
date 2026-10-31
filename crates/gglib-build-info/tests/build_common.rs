//! The tests of `crates/build_common.rs`, the README transformation every
//! crate's build script includes. A build script has no test target of its
//! own, so the file is included here, the way the build scripts include it.

#![allow(
    dead_code,
    reason = "the functions the build scripts call and these tests do not"
)]

use std::{env, fs, path::Path};

include!("../../build_common.rs");
