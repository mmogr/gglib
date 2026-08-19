//! Shape tests for [`super`]'s model-shaped DTOs.
//!
//! These build a domain [`Model`] and assert on what the DTO *serializes to*,
//! because the frontend reads JSON keys rather than Rust fields — a field that
//! exists on one DTO and not its sibling is invisible until a panel renders
//! the wrong number.
//!
//! [`Model`]: gglib_core::domain::Model

mod fixture {
    //! A domain [`Model`] with nothing optional recorded.
    //!
    //! `Model` has no `Default`, and every test below needs one, so the
    //! builder lives here and each module varies only the fields it cares
    //! about via struct-update syntax.

    use std::collections::HashMap;
    use std::path::PathBuf;

    use chrono::Utc;
    use gglib_core::ModelCapabilities;
    use gglib_core::domain::Model;

    pub(super) fn model() -> Model {
        Model {
            dialect_spec: None,
            id: 1,
            name: "test-model".to_owned(),
            model_key: String::new(),
            file_path: PathBuf::from("/models/test.gguf"),
            param_count_b: 7.0,
            architecture: None,
            quantization: None,
            context_length: None,
            expert_count: None,
            expert_used_count: None,
            expert_shared_count: None,
            metadata: HashMap::new(),
            added_at: Utc::now(),
            hf_repo_id: None,
            hf_commit_sha: None,
            hf_filename: None,
            download_date: None,
            last_update_check: None,
            tags: Vec::new(),
            capabilities: ModelCapabilities::default(),
            inference_defaults: None,
            defaults_origin: None,
            server_defaults: None,
            template_caps: None,
            benchmark_summary: None,
        }
    }
}

mod model_detail_template_caps_tests {
    //! The tri-state [`ModelDetailDto::reasoning_effort_support`] publishes.
    //!
    //! A client gates a UI control on this, and the whole reason it is not a
    //! `bool` is that "nobody has looked" must not render as "not supported" —
    //! ADR 0007 decision 3's rule, applied one layer out from the server's own
    //! suppression.

    use gglib_core::domain::{Model, Support, TemplateCaps};

    use super::super::ModelDetailDto;
    use super::fixture::model;

    fn support_of(model: Model) -> Support {
        ModelDetailDto::from_model(model, false, None).reasoning_effort_support
    }

    /// **The common row, and the one a `bool` would have got wrong.** Caps are
    /// read from `GET /props` while a model runs, so a model nobody has served
    /// on this installation has no reading at all — which licenses no
    /// conclusion in either direction.
    #[test]
    fn a_model_never_served_reports_unknown_not_unsupported() {
        assert_eq!(support_of(model()), Support::Unknown);
    }

    /// A caps object that arrived without the field is the same kind of
    /// nothing: five of the nine bools default `true` upstream, so an absent
    /// key is not a `false`.
    #[test]
    fn caps_that_omit_the_field_are_also_unknown() {
        let m = Model {
            template_caps: Some(TemplateCaps::default()),
            ..model()
        };
        assert_eq!(support_of(m), Support::Unknown);
    }

    #[test]
    fn a_template_that_reads_the_variable_reports_yes() {
        let m = Model {
            template_caps: Some(TemplateCaps {
                supports_reasoning_effort: Some(true),
                ..TemplateCaps::default()
            }),
            ..model()
        };
        assert_eq!(support_of(m), Support::Yes);
    }

    /// The one arm a surface may act on, and the one the server's own stage 5b
    /// acts on.
    #[test]
    fn a_positive_negative_reports_no() {
        let m = Model {
            template_caps: Some(TemplateCaps {
                supports_reasoning_effort: Some(false),
                ..TemplateCaps::default()
            }),
            ..model()
        };
        assert_eq!(support_of(m), Support::No);
    }

    /// The wire contract a client gates on. Asserted as JSON because that is
    /// what the client receives — three distinct strings, none of them a bool.
    #[test]
    fn the_three_states_serialize_distinguishably() {
        let mut seen = Vec::new();
        for caps in [
            None,
            Some(TemplateCaps {
                supports_reasoning_effort: Some(true),
                ..TemplateCaps::default()
            }),
            Some(TemplateCaps {
                supports_reasoning_effort: Some(false),
                ..TemplateCaps::default()
            }),
        ] {
            let dto = ModelDetailDto::from_model(
                Model {
                    template_caps: caps,
                    ..model()
                },
                false,
                None,
            );
            let json = serde_json::to_value(&dto).expect("serializes");
            seen.push(json["reasoningEffortSupport"].clone());
        }

        assert_eq!(
            seen,
            vec![
                serde_json::json!("unknown"),
                serde_json::json!("yes"),
                serde_json::json!("no"),
            ]
        );
    }

    /// The DTO is read back by the CLI's `--json` consumers, and an older
    /// payload has no such key. It must land on `unknown` — the one default
    /// that cannot cause a control to be hidden from a model that supports it.
    #[test]
    fn an_older_payload_without_the_key_reads_back_as_unknown() {
        let dto = ModelDetailDto::from_model(model(), false, None);
        let mut json = serde_json::to_value(&dto).expect("serializes");
        json.as_object_mut()
            .expect("an object")
            .remove("reasoningEffortSupport");

        let round_tripped: ModelDetailDto = serde_json::from_value(json).expect("deserializes");
        assert_eq!(round_tripped.reasoning_effort_support, Support::Unknown);
    }
}

mod gui_model_moe_tests {
    //! The MoE topology `GET /api/models` must carry.
    //!
    //! The library list renders *active* parameters, derived from
    //! `expertUsedCount / expertCount`. When those keys are absent the
    //! formatter silently falls back to the *total* — for a 128-expert model
    //! that is off by more than an order of magnitude, and it reads as a fact
    //! rather than a fallback. `ModelDetailDto` has carried the topology since
    //! it existed, so the inspector was right while the list beside it was
    //! wrong about the same model.

    use gglib_core::domain::Model;

    use super::super::{GuiModel, ModelDetailDto};
    use super::fixture::model;

    /// 128 experts, 8 active — the shape that makes the two views disagree.
    fn moe_model() -> Model {
        Model {
            expert_count: Some(128),
            expert_used_count: Some(8),
            expert_shared_count: Some(1),
            ..model()
        }
    }

    #[test]
    fn the_list_payload_carries_the_moe_topology() {
        let json = serde_json::to_value(GuiModel::from_model(moe_model(), false, None))
            .expect("serializes");

        assert_eq!(json["expertCount"], 128, "the list view divides by this");
        assert_eq!(json["expertUsedCount"], 8, "and multiplies by this");
        assert_eq!(json["expertSharedCount"], 1);
    }

    /// The keys are `skip_serializing_if`, so a dense model must not grow three
    /// nulls the frontend would have to spell a guard against.
    #[test]
    fn a_dense_model_omits_the_keys_entirely() {
        let json =
            serde_json::to_value(GuiModel::from_model(model(), false, None)).expect("serializes");

        for key in ["expertCount", "expertUsedCount", "expertSharedCount"] {
            assert!(
                json.get(key).is_none(),
                "dense model must omit {key}, got {:?}",
                json.get(key)
            );
        }
    }

    /// The invariant the bug violated: one model cannot have two parameter
    /// counts depending on which panel is looking at it.
    #[test]
    fn list_and_detail_agree_on_the_same_model() {
        let list =
            serde_json::to_value(GuiModel::from_model(moe_model(), false, None)).expect("list");
        let detail = serde_json::to_value(ModelDetailDto::from_model(moe_model(), false, None))
            .expect("detail");

        for key in ["expertCount", "expertUsedCount", "expertSharedCount"] {
            assert_eq!(
                list[key], detail[key],
                "{key} differs between the two views"
            );
        }
    }
}
