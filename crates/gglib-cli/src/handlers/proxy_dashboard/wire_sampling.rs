//! The sampling-readback half of the server's JSON contract.
//!
//! Split from [`super::wire`] rather than added to it, for the reason that file
//! gives about llama.cpp's `/slots`: a mirror grows with whatever the server
//! grows, and this half answers a different question from the rest of the
//! dashboard. Everything else there reports what the proxy *did*; this reports
//! what gglib **decided about two controls nothing echoes**.
//!
//! `Deserialize`-only, `#[serde(default)]` throughout and no
//! `deny_unknown_fields`, exactly like its sibling — `gglib proxy dashboard` is
//! routinely pointed at a proxy from a different build, in both directions.

use serde::Deserialize;

/// Mirror of the read parts of `gglib_proxy::sampling_audit::SamplingAuditSnapshot`.
///
/// Only the parts this dashboard renders. The divergence list and the `/props`
/// baseline have a surface already (the GUI panel), and mirroring them here
/// would be an obligation to keep two renderers in step for no reader.
#[derive(Debug, Default, Deserialize)]
pub(super) struct SamplingAudit {
    #[serde(default)]
    pub(super) reasoning: Option<Reasoning>,
    /// `None` from a proxy predating this field — which is not the same fact
    /// as an empty tally, and must not render as one. `sampling_audit` itself
    /// already ships in builds without `client_field_names`, so this is the
    /// ordinary skew case rather than a hypothetical.
    #[serde(default)]
    pub(super) client_field_names: Option<ClientFieldNames>,
}

/// Mirror of `gglib_proxy::audit_records::ReasoningReadback`.
///
/// The one section of this dashboard reporting something **no observation
/// backs**: llama-server echoes neither reasoning control, so what follows is
/// gglib's own record of what it sent. [`Self::wire_blind_reason`] arrives from
/// the server so this renderer never has to paraphrase the measurement.
#[derive(Debug, Default, Deserialize)]
pub(super) struct Reasoning {
    #[serde(default)]
    pub(super) effort_support: EffortSupport,
    #[serde(default)]
    pub(super) latest: Option<ResolvedReasoning>,
    #[serde(default)]
    pub(super) wire_blind_reason: String,
}

/// Mirror of `gglib_proxy::audit_records::EffortSupportState`.
///
/// The [`Self::Unrecognised`] arm keeps it a tri-state against a proxy newer
/// than this build: an answer this binary cannot read is another way of not
/// having observed one, and must never fall through to "the template does not
/// support it" — which would tell an old dashboard that every effort setting on
/// that model is inert.
#[derive(Debug, Default, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub(super) enum EffortSupport {
    Supported,
    NotSupported,
    NotYetObserved {
        #[serde(default)]
        reason: String,
    },
    #[default]
    #[serde(other)]
    Unrecognised,
}

/// Mirror of `gglib_proxy::audit_records::ResolvedReasoning`.
#[derive(Debug, Default, Deserialize)]
pub(super) struct ResolvedReasoning {
    #[serde(default)]
    pub(super) effort: Option<EffortRung>,
    #[serde(default)]
    pub(super) budget: Option<BudgetRung>,
}

/// Mirror of `gglib_proxy::audit_records::EffortRung`.
#[derive(Debug, Deserialize)]
pub(super) struct EffortRung {
    pub(super) level: String,
    #[serde(default)]
    pub(super) source: String,
    /// Whether the effort gate deleted it before sending. Rendering the level
    /// without this marker would report a control that went nowhere as though
    /// it had worked.
    #[serde(default)]
    pub(super) suppressed: bool,
}

/// Mirror of `gglib_proxy::audit_records::BudgetRung`.
#[derive(Debug, Deserialize)]
pub(super) struct BudgetRung {
    pub(super) tokens: i32,
    #[serde(default)]
    pub(super) source: String,
}

/// Mirror of `gglib_proxy::audit_records::ClientFieldNames`.
#[derive(Debug, Default, Deserialize)]
pub(super) struct ClientFieldNames {
    #[serde(default)]
    pub(super) fields: Vec<ClientFieldTally>,
    #[serde(default)]
    pub(super) untracked: u64,
}

/// Mirror of `gglib_proxy::audit_records::ClientFieldTally`.
#[derive(Debug, Deserialize)]
pub(super) struct ClientFieldTally {
    pub(super) field: String,
    #[serde(default)]
    pub(super) discarded: u64,
    #[serde(default)]
    pub(super) rejected: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **Unknown never becomes no, not even across builds.** A proxy newer than
    /// this binary can report a caps state it has never heard of, and the only
    /// safe reading of one is "not observed".
    #[test]
    fn an_unrecognised_support_state_is_not_read_as_unsupported() {
        let json = serde_json::json!({
            "effort_support": {"state": "supported_but_ignored_on_tuesdays"},
            "latest": null,
            "wire_blind_reason": "…"
        })
        .to_string();

        let got: Reasoning = serde_json::from_str(&json).expect("should deserialize");
        assert!(
            matches!(got.effort_support, EffortSupport::Unrecognised),
            "{:?}",
            got.effort_support
        );
    }

    /// The three states survive the round trip under the tags the server
    /// actually sends.
    #[test]
    fn the_three_support_states_round_trip() {
        let parse = |state: &str| {
            let json = format!(r#"{{"effort_support":{{"state":"{state}"}}}}"#);
            serde_json::from_str::<Reasoning>(&json)
                .expect("should deserialize")
                .effort_support
        };

        assert!(matches!(parse("supported"), EffortSupport::Supported));
        assert!(matches!(
            parse("not_supported"),
            EffortSupport::NotSupported
        ));
        assert!(matches!(
            parse("not_yet_observed"),
            EffortSupport::NotYetObserved { .. }
        ));
    }

    /// A proxy that predates the whole readback sends no object at all, which
    /// must not fail the frame the rest of the dashboard is drawn from.
    #[test]
    fn an_absent_readback_deserializes_to_its_empty_shape() {
        let got: SamplingAudit = serde_json::from_str("{}").expect("should deserialize");
        assert!(got.reasoning.is_none());
        assert!(got.client_field_names.is_none());
    }

    /// The skew case that matters, and the one a `#[serde(default)]` non-`Option`
    /// got wrong: `sampling_audit` ships in builds that predate
    /// `client_field_names`, so the tally is absent while the audit around it is
    /// present. Absent must stay absent — defaulting it to an empty tally is
    /// what let the renderer state that nothing had been dropped by a proxy
    /// that was in fact dropping every client sampler.
    #[test]
    fn an_audit_without_the_tally_reports_no_tally_rather_than_an_empty_one() {
        let got: SamplingAudit =
            serde_json::from_str(r#"{"reasoning":null}"#).expect("should deserialize");
        assert!(got.client_field_names.is_none());
    }
}
