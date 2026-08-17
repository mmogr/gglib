//! JSON-boundary tests for [`super`]'s request DTOs.
//!
//! These deserialize raw JSON strings rather than constructing the structs in
//! Rust, because the thing under test is the serde wiring itself.

mod update_model_request_tests {
    //! JSON-boundary tests for `UpdateModelRequest.server_defaults`.
    //!
    //! These deserialize raw JSON strings (rather than constructing the
    //! struct directly in Rust) to prove the `serde_with::rust::double_option`
    //! wiring actually distinguishes "field omitted" from "field explicitly
    //! null" at the layer where it matters — every other test for this
    //! feature bypassed serde entirely and would not have caught the
    //! original bug (double `Option` collapsing `null` into "omitted").

    use super::super::UpdateModelRequest;
    use gglib_core::domain::ServerConfig;

    #[test]
    fn server_defaults_omitted_key_is_none() {
        let req: UpdateModelRequest = serde_json::from_str("{}").unwrap();
        assert_eq!(
            req.server_defaults, None,
            "omitted key must resolve to None (no-op / don't touch)"
        );
    }

    #[test]
    fn server_defaults_explicit_null_is_some_none() {
        let req: UpdateModelRequest = serde_json::from_str(r#"{"serverDefaults": null}"#).unwrap();
        assert_eq!(
            req.server_defaults,
            Some(None),
            "explicit null must resolve to Some(None) (clear the override)"
        );
    }

    #[test]
    fn server_defaults_populated_object_is_some_some() {
        let req: UpdateModelRequest =
            serde_json::from_str(r#"{"serverDefaults": {"contextLength": 8192}}"#).unwrap();
        assert_eq!(
            req.server_defaults,
            Some(Some(ServerConfig {
                context_length: Some(8192)
            })),
            "populated object must resolve to Some(Some(config))"
        );
    }
}

mod start_server_request_tests {
    //! JSON-boundary tests for `StartServerRequest.mlock`.

    use super::super::StartServerRequest;

    #[test]
    fn mlock_deserializes_from_json() {
        let req: StartServerRequest = serde_json::from_str(r#"{"mlock": true}"#).unwrap();
        assert!(req.mlock, "explicit true must deserialize");
    }

    #[test]
    fn mlock_defaults_to_false_when_omitted() {
        let req: StartServerRequest = serde_json::from_str(r#"{}"#).unwrap();
        assert!(
            !req.mlock,
            "omitted key must default to false via #[serde(default)]"
        );
    }
}

mod model_detail_template_caps_tests {
    //! The tri-state [`ModelDetailDto::reasoning_effort_support`] publishes.
    //!
    //! A client gates a UI control on this, and the whole reason it is not a
    //! `bool` is that "nobody has looked" must not render as "not supported" —
    //! ADR 0007 decision 3's rule, applied one layer out from the server's own
    //! suppression.

    use std::collections::HashMap;
    use std::path::PathBuf;

    use chrono::Utc;
    use gglib_core::ModelCapabilities;
    use gglib_core::domain::{Model, Support, TemplateCaps};

    use super::super::ModelDetailDto;

    /// A model with nothing template-related recorded. `Model` has no
    /// `Default`, so the cases below build on this.
    fn model() -> Model {
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
