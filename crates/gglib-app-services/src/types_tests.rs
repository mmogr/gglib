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

mod update_settings_request_tests {
    //! `null` must clear a setting, for every field without exception.
    //!
    //! The struct's own doc comment promises "every field is
    //! `Option<Option<T>>` with `serde_with::rust::double_option`".
    //! `tool_call_repair` was declared bare, so serde collapsed its explicit
    //! `null` into the same `None` an omitted key produces: the GUI offered a
    //! clear-to-default the backend silently declined to perform. It is the
    //! same field whose unreachability motivated
    //! `scripts/check_settings_surfaces.sh` in the first place.

    use super::super::UpdateSettingsRequest;

    /// Three JSON states, three distinct meanings. Collapse any two and a
    /// setting becomes unclearable.
    #[test]
    fn tool_call_repair_distinguishes_all_three_states() {
        let omitted: UpdateSettingsRequest = serde_json::from_str("{}").expect("omitted");
        assert_eq!(
            omitted.tool_call_repair, None,
            "omitted key must mean leave unchanged"
        );

        let cleared: UpdateSettingsRequest =
            serde_json::from_str(r#"{"toolCallRepair": null}"#).expect("null");
        assert_eq!(
            cleared.tool_call_repair,
            Some(None),
            "explicit null must mean clear to default, not leave unchanged"
        );

        let set: UpdateSettingsRequest =
            serde_json::from_str(r#"{"toolCallRepair": false}"#).expect("false");
        assert_eq!(
            set.tool_call_repair,
            Some(Some(false)),
            "an explicit value must survive as itself"
        );
    }

    /// The clear must still be a clear after the hand-off to the domain
    /// update — the layer that decides what actually reaches storage.
    #[test]
    fn a_cleared_tool_call_repair_reaches_the_domain_update() {
        let request: UpdateSettingsRequest =
            serde_json::from_str(r#"{"toolCallRepair": null}"#).expect("null");
        let update: gglib_core::SettingsUpdate = request.into();

        assert_eq!(update.tool_call_repair, Some(None));
    }
}

mod mcp_server_info_shape_tests {
    //! What `POST /api/mcp/servers/{id}/start` and `/stop` actually answer.
    //!
    //! Both handlers return `McpServerInfo`; TypeScript declared the start
    //! route as `McpTool[]` and the stop route as `void`. The array
    //! declaration was wrong in a way nothing noticed, because a later
    //! `syncAllMcpTools()` did the work the returned value was supposed to
    //! do. This pins the envelope the frontend mirrors: tools arrive *inside*
    //! the server info, never instead of it.

    use super::super::McpServerInfo;

    #[test]
    fn the_response_is_an_envelope_not_a_tool_array() {
        let info: McpServerInfo = serde_json::from_value(serde_json::json!({
            "server": {
                "id": 1,
                "name": "test",
                "server_type": "stdio",
                "config": {},
                "enabled": true,
                "lifecycle": "lazy",
                "env": [],
                "created_at": "2024-01-01T00:00:00Z",
                "is_valid": true,
            },
            "status": "running",
            "tools": [],
        }))
        .expect("deserializes");

        let json = serde_json::to_value(&info).expect("serializes");

        assert!(json.is_object(), "the response is an object, not an array");
        for key in ["server", "status", "tools"] {
            assert!(json.get(key).is_some(), "missing {key}");
        }
        assert!(json["tools"].is_array(), "tools live inside the envelope");
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
