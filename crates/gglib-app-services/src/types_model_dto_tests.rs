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
            components: Vec::new(),
            image_family: None,
            id: 1,
            name: "test-model".to_owned(),
            model_key: String::new(),
            file_path: PathBuf::from("/models/test.gguf"),
            projector_path: None,
            param_count_b: Some(7.0),
            architecture: None,
            variant_label: None,
            file_type: None,
            bits_per_weight: None,
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
    //! ADR 0007 decision 3's rule.

    use gglib_core::domain::{Model, ModelDetailDto, Support, TemplateCaps};

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

    /// The one arm a surface may act on.
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

mod model_detail_effort_levels_tests {
    //! The effort levels [`ModelDetailDto::reasoning_effort_levels`] serves
    //! from the stored caps: the template's list as gglib's levels, and no
    //! list at all when none is stored.

    use gglib_core::domain::{Model, ModelDetailDto, ReasoningEffort, TemplateCaps};
    use serde_json::{Value, json};

    use super::fixture::model;

    /// A model whose stored caps list `levels`.
    fn listing(levels: &[&str]) -> Model {
        Model {
            template_caps: Some(TemplateCaps {
                supports_reasoning_effort: Some(true),
                reasoning_effort_levels: Some(levels.iter().map(|&l| l.to_owned()).collect()),
                ..TemplateCaps::default()
            }),
            ..model()
        }
    }

    /// The detail's JSON, as the window reads it.
    fn json_of(model: Model) -> Value {
        serde_json::to_value(ModelDetailDto::from_model(model, false, None)).expect("serializes")
    }

    /// Qwen3.8's list: the three levels it renders, and no other.
    #[test]
    fn a_stored_list_is_served_as_its_levels() {
        let detail = ModelDetailDto::from_model(listing(&["low", "medium", "xhigh"]), false, None);

        assert_eq!(
            detail.reasoning_effort_levels,
            Some(vec![
                ReasoningEffort::Low,
                ReasoningEffort::Medium,
                ReasoningEffort::XHigh
            ])
        );
        assert_eq!(
            json_of(listing(&["low", "medium", "xhigh"]))["reasoningEffortLevels"],
            json!(["low", "medium", "xhigh"])
        );
    }

    /// No stored list is unknown, and unknown is no key: a model never
    /// launched here, and caps from a server too old to report the list.
    #[test]
    fn no_stored_list_is_served_as_no_list() {
        let old_server = Model {
            template_caps: Some(TemplateCaps {
                supports_reasoning_effort: Some(true),
                ..TemplateCaps::default()
            }),
            ..model()
        };

        for unknown in [model(), old_server] {
            let json = json_of(unknown);
            assert_eq!(json.get("reasoningEffortLevels"), None, "{json}");
        }
    }

    /// An empty list is a stored answer, not a missing one, so it travels.
    #[test]
    fn an_empty_list_is_served_empty() {
        assert_eq!(json_of(listing(&[]))["reasoningEffortLevels"], json!([]));
    }

    /// A word no control could send is left out; the levels around it stay.
    #[test]
    fn a_word_that_is_not_a_level_is_left_out() {
        let detail = ModelDetailDto::from_model(listing(&["low", "turbo", "max"]), false, None);

        assert_eq!(
            detail.reasoning_effort_levels,
            Some(vec![ReasoningEffort::Low, ReasoningEffort::Max])
        );
    }

    /// A payload without the key, as the CLI reads `--json` back, has no list.
    #[test]
    fn a_payload_without_the_key_reads_back_as_no_list() {
        let mut json = json_of(listing(&["low"]));
        json.as_object_mut()
            .expect("an object")
            .remove("reasoningEffortLevels");

        let round_tripped: ModelDetailDto = serde_json::from_value(json).expect("deserializes");
        assert_eq!(round_tripped.reasoning_effort_levels, None);
    }
}

mod gui_model_moe_tests {
    //! The `MoE` topology `GET /api/models` must carry.
    //!
    //! The library list renders *active* parameters, derived from
    //! `expertUsedCount / expertCount`. When those keys are absent the
    //! formatter silently falls back to the *total* — for a 128-expert model
    //! that is off by more than an order of magnitude, and it reads as a fact
    //! rather than a fallback. `ModelDetailDto` has carried the topology since
    //! it existed, so the inspector was right while the list beside it was
    //! wrong about the same model.

    use gglib_core::domain::{Model, ModelDetailDto};

    use super::super::GuiModel;
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

mod image_model_tests {
    //! What the list row and the detail say about a model that draws: its
    //! family, its component links and the roles it still needs.

    use gglib_core::domain::{ComponentRole, ImageFamily, Model, ModelComponent, ModelDetailDto};
    use serde_json::json;

    use super::fixture::model;
    use crate::types::GuiModel;

    /// A Flux.1 model linked to a VAE that is on disk and a CLIP-L that is
    /// not.
    fn flux(vae: &std::path::Path) -> Model {
        Model {
            image_family: Some(ImageFamily::Flux1),
            components: vec![
                ModelComponent {
                    role: ComponentRole::Vae,
                    path: vae.to_path_buf(),
                },
                ModelComponent {
                    role: ComponentRole::ClipL,
                    path: "/gone/clip_l.safetensors".into(),
                },
            ],
            ..model()
        }
    }

    #[test]
    fn a_model_that_draws_names_its_family_links_and_missing_roles() {
        let dir = tempfile::tempdir().unwrap();
        let vae = dir.path().join("ae.safetensors");
        std::fs::write(&vae, b"vae").unwrap();

        let list = serde_json::to_value(GuiModel::from_model(flux(&vae), false, None)).unwrap();
        let detail =
            serde_json::to_value(ModelDetailDto::from_model(flux(&vae), false, None)).unwrap();

        assert_eq!(list["imageFamily"], "flux1");
        assert_eq!(list["missingComponents"], json!(["t5xxl"]));
        // Its size is every file it has on disk: the VAE's three bytes, as
        // the weights and the CLIP-L are not there.
        assert_eq!(list["sizeBytes"], 3);
        assert_eq!(detail["imageFamily"], "flux1");
        assert_eq!(detail["missingComponents"], json!(["t5xxl"]));
        assert_eq!(
            detail["components"],
            json!([
                { "role": "vae", "path": vae.to_string_lossy(), "present": true },
                { "role": "clip_l", "path": "/gone/clip_l.safetensors", "present": false },
            ])
        );
    }

    /// A model that chats sends no family and empty lists, never nulls.
    #[test]
    fn a_model_that_chats_sends_no_family_and_empty_lists() {
        let list = serde_json::to_value(GuiModel::from_model(model(), false, None)).unwrap();
        let detail =
            serde_json::to_value(ModelDetailDto::from_model(model(), false, None)).unwrap();

        assert!(list.get("imageFamily").is_none(), "{list}");
        assert_eq!(list["missingComponents"], json!([]));
        assert_eq!(list["sizeBytes"], 0, "no file on disk");
        assert!(detail.get("imageFamily").is_none(), "{detail}");
        assert_eq!(detail["components"], json!([]));
        assert_eq!(detail["missingComponents"], json!([]));
    }
}
