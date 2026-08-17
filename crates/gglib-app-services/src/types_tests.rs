//! JSON-boundary tests for [`super`]'s request and response envelopes.
//!
//! These work in raw JSON rather than constructing the structs in Rust,
//! because the thing under test is the serde wiring itself. Tests for the
//! model-shaped DTOs live beside them in `types_model_dto_tests.rs`.

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
