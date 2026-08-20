//! The sampling flag surface: that it is exactly what `SamplingArgs`
//! declares, and that every per-run flag has a per-model twin to be stored in.
//!
//! Split from `cli_parity.rs`, which guards that `serve` and `proxy` agree
//! about the rest of their flags. The two ask different questions — that one
//! compares two commands to each other, this one compares a command to the
//! struct it is built from — and together they were over the file budget.
//!
//! # The expectation is derived, not transcribed
//!
//! [`sampling_flags`] asks clap what `SamplingArgs` declares. The literal it
//! replaced listed seven of fifteen flags — its own doc said so — and since
//! every assertion here is a `contains`, that guard passed while eight flags
//! could have vanished from either command without a word. A derived set
//! cannot drift from the struct.
//!
//! # Why the other direction needs a transcribed complement
//!
//! "No extra sampling flag" cannot be asked of the derived set alone. An
//! inline `--mirostat` on `Serve` is, by definition, a flag `SamplingArgs`
//! does not declare, so any comparison that first filters the command's flags
//! down to the derived ones has already discarded it — the comparison that
//! remains is `expected == expected`, which is true whatever the command
//! carries. (It was written that way once, and it was: a `--mirostat` added to
//! `Serve` alone passed every test.)
//!
//! Closing it needs a second set that says what the rest of the surface is.
//! [`SERVE_NON_SAMPLING`] and [`PROXY_NON_SAMPLING`] are that set, and they
//! are the only transcribed lists left. The assertion is then over the *whole*
//! flag surface: `SamplingArgs` plus the named remainder, exactly. A new flag
//! on either command fails until someone classifies it — which is the
//! mechanism, not a nuisance, because until it is classified a new flag and a
//! sampling parameter gone back inline look identical.

#[path = "support/flag_surface.rs"]
mod flag_surface;

use flag_surface::{assert_both_expose, long_flags, long_flags_at, owned, sampling_flags};

/// Every long flag on `gglib serve` that does not come from `SamplingArgs`:
/// `ContextArgs`, `ServeOptions`, `MtpArgs`, `CacheArgs` and `AccessArgs`.
///
/// Transcribed, because it is the complement of a derived set and nothing can
/// derive it — see the module docs. Keeping it current is the price of the
/// only assertion that can see an extra sampling flag.
const SERVE_NON_SAMPLING: &[&str] = &[
    "allowed-host",
    // Selects a named profile; not itself a sampling parameter. Deliberately
    // absent from PROXY_NON_SAMPLING: an unpinned proxy has no model in scope
    // for a default to attach to.
    "profile",
    "api-key",
    "cache",
    "cache-disk-gb",
    "ctx-size",
    "host",
    "jinja",
    "mlock",
    "mtp-draft-n-max",
    "mtp-draft-p-min",
    "port",
    "slot-dir",
];

/// The same for `gglib proxy`: `CacheArgs`, `AccessArgs`, and the three it
/// declares inline. No context or MTP flags — a standalone proxy has no
/// specific model in scope, so `--default-context` stands in for `--ctx-size`.
const PROXY_NON_SAMPLING: &[&str] = &[
    "allowed-host",
    "api-key",
    "cache",
    "cache-disk-gb",
    "default-context",
    "host",
    "port",
    "slot-dir",
];

/// Both directions: neither command may be missing a `SamplingArgs` flag, and
/// neither may have grown a flag that is in neither `SamplingArgs` nor its own
/// non-sampling list — which is what a field gone back inline looks like.
///
/// The second half compares the command's *entire* flag surface, not its
/// sampling-shaped subset. Filtering to the derived set first is what makes
/// the extra direction unaskable; the module docs work through why.
#[test]
fn serve_and_proxy_expose_exactly_the_sampling_flags_the_struct_declares() {
    let expected = sampling_flags();
    assert!(
        expected.len() >= 17,
        "derivation looks broken; got {expected:?}"
    );
    assert_both_expose(&expected, "sampling");

    for (command, rest) in [("serve", SERVE_NON_SAMPLING), ("proxy", PROXY_NON_SAMPLING)] {
        let mut whole = expected.clone();
        whole.extend(owned(rest));
        whole.sort();

        assert_eq!(
            long_flags(command),
            whole,
            "`{command}`'s flag surface is not SamplingArgs plus its declared \
             non-sampling flags. An unexpected flag is either a sampling \
             parameter declared inline instead of on SamplingArgs, or a new \
             flag that has to be named in {command}'s list here"
        );
    }
}

/// Every per-run sampling flag must have a per-model twin on `gglib model
/// update`, which is where the same parameter is *stored*.
///
/// This half of the surface was entirely unguarded: the twins are a second,
/// hand-written copy of the same flags in `ModelCommand::Update`, and nothing
/// compared the two lists. A parameter that can be passed for one run but
/// never saved is a hole in the ladder — the value has nowhere to live
/// between invocations.
///
/// `--seed` is the sole deliberate asymmetry and is absent from both, so it
/// needs no exemption here: a stored seed would pin every response a model
/// produced to the same text.
#[test]
fn model_update_exposes_a_twin_of_every_sampling_flag() {
    let update = long_flags_at(&["model", "update"]);

    for flag in sampling_flags() {
        assert!(
            update.contains(&flag),
            "`gglib model update` cannot store --{flag}; it has {update:?}"
        );
    }
}

/// Storing a value is only half of it. Without a per-parameter clear, dialling
/// one stored default back to *unset* is inexpressible: the flags all carry
/// values, an omitted flag means "not mentioned", and
/// `--clear-inference-defaults` is all-or-nothing.
#[test]
fn model_update_can_clear_one_stored_parameter() {
    let update = long_flags_at(&["model", "update"]);

    assert!(update.contains(&"unset".to_owned()), "got {update:?}");
    assert!(
        update.contains(&"clear-inference-defaults".to_owned()),
        "the all-or-nothing form stays: {update:?}"
    );
}
