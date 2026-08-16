//! The template-capability half of a `GET /props` read.
//!
//! [`crate::props`] reads one endpoint that carries two independent facts:
//! the build's default sampler table (`default_generation_settings.params`,
//! the ADR 0003 baseline) and the loaded template's capability self-report
//! (`chat_template_caps`, ADR 0007). They used to be collapsed into one
//! all-or-nothing result, which discarded caps present in a body whose
//! `params` was missing — this module exists to hold the two halves apart.
//!
//! A separate module rather than more of `props.rs` for the same reason
//! `props_parse_tests.rs` is: `props.rs` sits at its file budget, and the
//! caps half is a distinct observation with its own ADR behind it.
//!
//! # One read, one latch
//!
//! Both halves ride the poller's single per-launch `/props` read (see
//! `slots_poller::BaselineLatch`): the caps are computed once per template
//! load and published unconditionally on the pinned build — measured, they
//! are byte-identical across `--jinja`, `--no-jinja` and the flagless launch
//! — so a read that yields the baseline has also yielded whatever caps the
//! build publishes, and there is nothing left to retry for separately.

use gglib_core::domain::{TemplateCaps, TemplateCapsState};

use crate::props::PropsResult;

/// Both halves of one `/props` read, independently optional.
///
/// The type this module exists for: a body carrying caps but no params — a
/// real shape, one of the parse fixtures — must yield
/// [`PropsResult::Unavailable`] *and* [`TemplateCapsState::Read`], not one
/// collapsed failure.
#[derive(Debug, Clone, PartialEq)]
pub struct PropsReading {
    /// The default-sampler-table half. See [`crate::props`].
    pub params: PropsResult,
    /// The template-capability half. See
    /// [`gglib_core::domain::TemplateCaps`].
    pub caps: TemplateCapsState,
}

impl PropsReading {
    /// A read that produced nothing at all — HTTP failure, non-success
    /// status, or an unparseable body. Both halves carry the same reason,
    /// because both were lost to the same cause.
    pub(crate) fn unreadable(reason: impl Into<String>) -> Self {
        let reason = reason.into();
        Self {
            params: PropsResult::Unavailable(reason.clone()),
            caps: TemplateCapsState::Unreadable { reason },
        }
    }

    /// A parsed body, each section present or absent on its own.
    ///
    /// An empty caps object (`{}`) is stored as [`TemplateCapsState::Read`]
    /// with every field `None` — never nine `false`s: five of the nine
    /// default `true` upstream, so absence licenses nothing, and the
    /// per-field `Option` is what keeps `reasoning_effort_support` answering
    /// `Unknown` for it.
    pub(crate) fn of(
        params: Option<crate::sampling_audit::SlotParams>,
        caps: Option<TemplateCaps>,
    ) -> Self {
        Self {
            params: params.map_or_else(
                || {
                    PropsResult::Unavailable(
                        "no default_generation_settings.params in /props".to_string(),
                    )
                },
                PropsResult::Available,
            ),
            caps: caps.map_or_else(
                || TemplateCapsState::Unreadable {
                    reason: "no chat_template_caps in /props (pre-caps llama-server build?)"
                        .to_string(),
                },
                |caps| TemplateCapsState::Read { caps },
            ),
        }
    }
}
