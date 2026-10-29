#![doc = include_str!(concat!(env!("OUT_DIR"), "/README_GENERATED.md"))]

//! The one Swift-facing surface of gglib.
//!
//! It is the crate through which an app reaches gglib's own code, built as
//! `GglibFFI.framework` and bound by the Swift that `uniffi` generates from
//! it. Chat itself never crosses this boundary: an app talks to a gglib it
//! serves over loopback HTTP, as it talks to one on another machine.
//!
//! # The shape the caller sees
//!
//! ```text
//! build_info() -> BuildInfo
//!     BuildInfo.gglib_version    -> String   the workspace version
//!     BuildInfo.uniffi_version   -> String   the uniffi that built the binding
//!     BuildInfo.llama_pin        -> String   PINNED_LLAMA_RELEASE
//!
//! open_library(LibraryDirs, hf_token?) async throws -> Library
//!     Library.suggested(DeviceReading)            -> [Suggestion]   no network
//!     Library.search(query, cursor?)              throws -> SearchPage
//!     Library.quantizations(repo, DeviceReading)  throws -> [QuantOffer]
//!     Library.installed()                         throws -> [InstalledModel]
//!     Library.remove(id)                          throws
//!     Library.download(repo, quantization)        throws -> DownloadWatch
//!     Library.pause_all() / Library.resume_all()
//!         DownloadWatch.next()    -> DownloadProgress?   None once it is over
//!         DownloadWatch.cancel()                         returns at once
//! ```
//!
//! Every `Library` call but `suggested`, and `DownloadWatch.next`, is
//! `async`; every failure is a [`GglibError`]. A verdict ([`FitVerdict`], with its [`FitWhy`]) is
//! gglib-core's, judged against the budget gglib makes from the
//! [`DeviceReading`] the app took.
//!
//! # One module name
//!
//! `GglibFFI` names the framework, its binary, the `framework module` in its
//! `Modules/module.modulemap` and the C module the generated Swift imports.
//! `uniffi.toml` sets the last, and the framework build passes the same name
//! to the generator, so a consumer imports one module whichever way it finds
//! the framework.
//!
//! # The runtime, and who owns it
//!
//! A decision with alternatives, recorded here rather than left to the code.
//! An app links a library and calls a function: there is no
//! `#[tokio::main]` above it, and `uniffi` polls an exported future on the
//! Swift thread that awaits it, with no tokio context. Something has to own
//! the worker threads, and something has to make them current.
//!
//! **This library owns one, process-wide, built on first use** (`runtime`):
//! two workers, multi-threaded, never torn down. Every async export wraps
//! its body in that module's `in_runtime`: the body runs on the calling
//! thread with the runtime entered, and the tasks it spawns run on the two
//! workers. The alternatives, and why each is not it:
//!
//! - **Not per call.** Work a call starts (a download, a server) outlives
//!   the call. A runtime dropped when the call returns takes those tasks
//!   with it.
//! - **Not `uniffi`'s `async_runtime = "tokio"`.** That wraps each exported
//!   future in async-compat's `Compat`, which enters the polling thread's
//!   runtime if it has one and otherwise a single-threaded runtime of its
//!   own. A Swift thread never has one, so every task, timer and socket a
//!   call started would run on that one thread. `uniffi`'s `tokio` feature
//!   is off, so that attribute does not compile here.
//! - **Not torn down when a caller is done.** One served engine stopping
//!   must not stop a download another screen started. Two idle threads for
//!   the life of the process are the price.
//! - **Async across the boundary, not a callback interface.** `uniffi`
//!   bridges an `async fn` to a Swift `async` call, so structured concurrency
//!   and actor hopping work as the app expects. Cancelling the Swift `Task`
//!   does not reach Rust (`uniffi`'s Swift never calls `rust_future_cancel`),
//!   so a wait the app must be able to end is an object with a `cancel`, and
//!   a stream is pulled one `next()` at a time.

mod browse;
mod build_info;
mod download_feed;
mod download_watch;
mod downloads;
mod errors;
mod installed;
mod library;
mod reading;
mod runtime;
mod verdict;

#[cfg(test)]
mod test_gguf;
#[cfg(test)]
mod test_hub;
#[cfg(test)]
mod test_library;
#[cfg(test)]
mod test_server;

pub use browse::{QuantOffer, SearchHit, SearchPage, Suggestion};
pub use build_info::{BuildInfo, build_info};
pub use download_watch::{DownloadProgress, DownloadState, DownloadWatch};
pub use errors::GglibError;
pub use installed::InstalledModel;
pub use library::{Library, LibraryDirs, open_library};
pub use reading::DeviceReading;
pub use verdict::{FitVerdict, FitWhy, MissingInput};

// Defines `UniFfiTag` and the `extern "C"` entry points, at the crate root
// where every `uniffi` derive and export resolves that tag.
//
// No `allow(unsafe_code)`, here or anywhere in this crate, and none is
// needed: rustc does not report `unsafe_code` inside the expansion of another
// crate's macro, so the `#[unsafe(no_mangle)]` entry points `uniffi` writes
// compile under the workspace's deny, while the same line written by hand in
// this crate does not. Every `unsafe` here is one `uniffi` generated.
uniffi::setup_scaffolding!();
